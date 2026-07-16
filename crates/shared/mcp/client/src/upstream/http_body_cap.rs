use std::{borrow::Cow, collections::HashMap, sync::Arc};

use futures::{stream::BoxStream, StreamExt};
use http::{HeaderName, HeaderValue};
use reqwest::header::{ACCEPT, WWW_AUTHENTICATE};
use rmcp::{
    model::{ClientJsonRpcMessage, JsonRpcMessage, ServerJsonRpcMessage},
    transport::{
        common::http_header::{
            EVENT_STREAM_MIME_TYPE, HEADER_LAST_EVENT_ID, HEADER_MCP_PROTOCOL_VERSION,
            HEADER_SESSION_ID, JSON_MIME_TYPE,
        },
        streamable_http_client::{
            AuthRequiredError, InsufficientScopeError, SseError, StreamableHttpClient,
            StreamableHttpError, StreamableHttpPostResponse,
        },
    },
};
use sse_stream::{Sse, SseStream};

#[derive(Clone)]
pub struct BodyCappedHttpClient {
    inner: reqwest::Client,
    json_max_bytes: usize,
    sse_event_max_bytes: usize,
}

impl BodyCappedHttpClient {
    #[must_use]
    pub fn new(inner: reqwest::Client, json_max_bytes: usize, sse_event_max_bytes: usize) -> Self {
        Self {
            inner,
            json_max_bytes,
            sse_event_max_bytes,
        }
    }

    #[must_use]
    pub fn default_with_caps(json_max_bytes: usize, sse_event_max_bytes: usize) -> Self {
        let inner = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build gateway HTTP client");
        Self::new(inner, json_max_bytes, sse_event_max_bytes)
    }
}

impl StreamableHttpClient for BodyCappedHttpClient {
    type Error = reqwest::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let session_was_attached = session_id.is_some();
        let mut request = self
            .inner
            .post(uri.as_ref())
            .header(ACCEPT, [EVENT_STREAM_MIME_TYPE, JSON_MIME_TYPE].join(", "));
        if let Some(token) = auth_token {
            request = request.bearer_auth(token);
        }
        if let Some(session_id) = session_id {
            request = request.header(HEADER_SESSION_ID, session_id.as_ref());
        }
        let response = apply_custom_headers(request, custom_headers)?
            .json(&message)
            .send()
            .await
            .map_err(StreamableHttpError::Client)?;
        response_to_post_result(response, message, session_was_attached, self.json_max_bytes).await
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let mut request = self
            .inner
            .delete(uri.as_ref())
            .header(HEADER_SESSION_ID, session_id.as_ref());
        if let Some(token) = auth_token {
            request = request.bearer_auth(token);
        }
        let response = apply_custom_headers(request, custom_headers)?
            .send()
            .await
            .map_err(StreamableHttpError::Client)?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Err(StreamableHttpError::ServerDoesNotSupportDeleteSession);
        }
        response
            .error_for_status()
            .map(|_| ())
            .map_err(StreamableHttpError::Client)
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        last_event_id: Option<String>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        let mut request = self
            .inner
            .get(uri.as_ref())
            .header(ACCEPT, [EVENT_STREAM_MIME_TYPE, JSON_MIME_TYPE].join(", "))
            .header(HEADER_SESSION_ID, session_id.as_ref());
        if let Some(last_event_id) = last_event_id {
            request = request.header(HEADER_LAST_EVENT_ID, last_event_id);
        }
        if let Some(token) = auth_token {
            request = request.bearer_auth(token);
        }
        let response = apply_custom_headers(request, custom_headers)?
            .send()
            .await
            .map_err(StreamableHttpError::Client)?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Err(StreamableHttpError::ServerDoesNotSupportSse);
        }
        let response = response
            .error_for_status()
            .map_err(StreamableHttpError::Client)?;
        ensure_stream_content_type(&response)?;
        let capped = per_event_capped_stream(response.bytes_stream(), self.sse_event_max_bytes);
        Ok(SseStream::from_bytes_stream(capped).boxed())
    }
}

fn apply_custom_headers(
    mut request: reqwest::RequestBuilder,
    custom_headers: HashMap<HeaderName, HeaderValue>,
) -> Result<reqwest::RequestBuilder, StreamableHttpError<reqwest::Error>> {
    for (name, value) in custom_headers {
        validate_custom_header(&name).map_err(StreamableHttpError::ReservedHeaderConflict)?;
        request = request.header(name, value);
    }
    Ok(request)
}

fn validate_custom_header(name: &HeaderName) -> Result<(), String> {
    let reserved = [
        "accept",
        HEADER_SESSION_ID,
        HEADER_LAST_EVENT_ID,
        HEADER_MCP_PROTOCOL_VERSION,
    ];
    if reserved
        .iter()
        .any(|reserved| name.as_str().eq_ignore_ascii_case(reserved))
        && !name
            .as_str()
            .eq_ignore_ascii_case(HEADER_MCP_PROTOCOL_VERSION)
    {
        return Err(name.to_string());
    }
    Ok(())
}

async fn response_to_post_result(
    response: reqwest::Response,
    message: ClientJsonRpcMessage,
    session_was_attached: bool,
    max_bytes: usize,
) -> Result<StreamableHttpPostResponse, StreamableHttpError<reqwest::Error>> {
    if let Some(error) = auth_error(&response) {
        return Err(error);
    }
    let status = response.status();
    if matches!(
        status,
        reqwest::StatusCode::ACCEPTED | reqwest::StatusCode::NO_CONTENT
    ) {
        return Ok(StreamableHttpPostResponse::Accepted);
    }
    if status == reqwest::StatusCode::NOT_FOUND && session_was_attached {
        return Err(StreamableHttpError::SessionExpired);
    }
    let content_type = content_type(&response);
    let session_id = response
        .headers()
        .get(HEADER_SESSION_ID)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    if status.is_success() && response.content_length() == Some(0) && is_empty_ok(&message) {
        return Ok(StreamableHttpPostResponse::Accepted);
    }
    if !status.is_success() {
        return non_success_response(status, content_type, response, max_bytes).await;
    }
    match content_type.as_deref() {
        Some(ct) if ct.as_bytes().starts_with(EVENT_STREAM_MIME_TYPE.as_bytes()) => {
            let capped = per_event_capped_stream(response.bytes_stream(), max_bytes);
            Ok(StreamableHttpPostResponse::Sse(
                SseStream::from_bytes_stream(capped).boxed(),
                session_id,
            ))
        }
        Some(ct) if ct.as_bytes().starts_with(JSON_MIME_TYPE.as_bytes()) => {
            let bytes = read_body_capped(response, max_bytes).await?;
            match serde_json::from_slice::<ServerJsonRpcMessage>(&bytes) {
                Ok(message) => Ok(StreamableHttpPostResponse::Json(message, session_id)),
                Err(_) => Ok(StreamableHttpPostResponse::Accepted),
            }
        }
        _ => Err(StreamableHttpError::UnexpectedContentType(content_type)),
    }
}

fn auth_error(response: &reqwest::Response) -> Option<StreamableHttpError<reqwest::Error>> {
    let header = response.headers().get(WWW_AUTHENTICATE)?.to_str().ok()?;
    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED => Some(StreamableHttpError::AuthRequired(
            AuthRequiredError::new(header.to_owned()),
        )),
        reqwest::StatusCode::FORBIDDEN => Some(StreamableHttpError::InsufficientScope(
            InsufficientScopeError::new(header.to_owned(), extract_scope(header)),
        )),
        _ => None,
    }
}

async fn non_success_response(
    status: reqwest::StatusCode,
    content_type: Option<String>,
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<StreamableHttpPostResponse, StreamableHttpError<reqwest::Error>> {
    let bytes = read_body_capped(response, max_bytes).await?;
    let body = String::from_utf8_lossy(&bytes);
    if content_type
        .as_deref()
        .is_some_and(|ct| ct.as_bytes().starts_with(JSON_MIME_TYPE.as_bytes()))
    {
        if let Some(message) = parse_json_rpc_error(&body) {
            return Ok(StreamableHttpPostResponse::Json(message, None));
        }
    }
    Err(StreamableHttpError::UnexpectedServerResponse(Cow::Owned(
        format!("HTTP {status}: {body}"),
    )))
}

async fn read_body_capped(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, StreamableHttpError<reqwest::Error>> {
    if let Some(length) = response.content_length() {
        if length > max_bytes as u64 {
            return Err(too_large(format!(
                "response_too_large: declared {length} bytes, max {max_bytes}"
            )));
        }
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(StreamableHttpError::Client)?;
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(too_large(format!(
                "response_too_large: streamed {} bytes, max {max_bytes}",
                bytes.len() + chunk.len()
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn per_event_capped_stream(
    inner: impl futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
    max_bytes: usize,
) -> BoxStream<'static, Result<bytes::Bytes, CappedStreamError>> {
    inner
        .scan((0usize, false), move |state, item| {
            let result = match item {
                Ok(chunk) => account_event_bytes(&chunk, state, max_bytes).map(|_| chunk),
                Err(error) => Err(CappedStreamError::Reqwest(error)),
            };
            futures::future::ready(Some(result))
        })
        .boxed()
}

fn account_event_bytes(
    chunk: &[u8],
    state: &mut (usize, bool),
    max_bytes: usize,
) -> Result<(), CappedStreamError> {
    for byte in chunk {
        if state.1 && *byte == b'\n' {
            state.0 = 0;
            state.1 = false;
            continue;
        }
        state.0 = state.0.saturating_add(1);
        if state.0 > max_bytes {
            return Err(CappedStreamError::TooLarge {
                event_bytes: state.0,
                max_bytes,
            });
        }
        state.1 = *byte == b'\n';
    }
    Ok(())
}

#[derive(Debug)]
enum CappedStreamError {
    Reqwest(reqwest::Error),
    TooLarge {
        event_bytes: usize,
        max_bytes: usize,
    },
}

impl std::fmt::Display for CappedStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reqwest(error) => write!(f, "upstream stream error: {error}"),
            Self::TooLarge {
                event_bytes,
                max_bytes,
            } => write!(
                f,
                "response_too_large: single SSE event reached {event_bytes} bytes, max {max_bytes}"
            ),
        }
    }
}

impl std::error::Error for CappedStreamError {}

fn ensure_stream_content_type(
    response: &reqwest::Response,
) -> Result<(), StreamableHttpError<reqwest::Error>> {
    match response.headers().get(reqwest::header::CONTENT_TYPE) {
        Some(value) => {
            let raw = value.as_bytes();
            if raw.starts_with(EVENT_STREAM_MIME_TYPE.as_bytes())
                || raw.starts_with(JSON_MIME_TYPE.as_bytes())
            {
                Ok(())
            } else {
                Err(StreamableHttpError::UnexpectedContentType(Some(
                    String::from_utf8_lossy(raw).to_string(),
                )))
            }
        }
        None => Err(StreamableHttpError::UnexpectedContentType(None)),
    }
}

fn content_type(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .map(|value| String::from_utf8_lossy(value.as_bytes()).to_string())
}

fn is_empty_ok(message: &ClientJsonRpcMessage) -> bool {
    matches!(
        message,
        ClientJsonRpcMessage::Notification(_)
            | ClientJsonRpcMessage::Response(_)
            | ClientJsonRpcMessage::Error(_)
    )
}

fn parse_json_rpc_error(body: &str) -> Option<ServerJsonRpcMessage> {
    match serde_json::from_str::<ServerJsonRpcMessage>(body) {
        Ok(message @ JsonRpcMessage::Error(_)) => Some(message),
        _ => None,
    }
}

fn extract_scope(header: &str) -> Option<String> {
    let lower = header.to_ascii_lowercase();
    let start = lower.find("scope=")? + "scope=".len();
    let value = &header[start..];
    if let Some(quoted) = value.strip_prefix('"') {
        return quoted.split('"').next().map(ToOwned::to_owned);
    }
    let end = value
        .find(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .unwrap_or(value.len());
    (end > 0).then(|| value[..end].to_owned())
}

fn too_large(message: String) -> StreamableHttpError<reqwest::Error> {
    StreamableHttpError::UnexpectedServerResponse(Cow::Owned(message))
}

#[cfg(test)]
#[path = "http_body_cap_tests.rs"]
mod tests;
