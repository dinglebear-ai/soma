use serde_json::Value;

use super::{CapScope, ResponseCaps, TransportKind, UpstreamError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpTransportDecision {
    Json,
    Sse,
    WebSocket,
}

pub fn decide_http_transport(url: &str) -> HttpTransportDecision {
    if url.starts_with("ws://") || url.starts_with("wss://") {
        return HttpTransportDecision::WebSocket;
    }
    if url.contains("transport=sse") {
        return HttpTransportDecision::Sse;
    }
    HttpTransportDecision::Json
}

pub fn transport_kind_for_decision(decision: &HttpTransportDecision) -> TransportKind {
    match decision {
        HttpTransportDecision::Json => TransportKind::HttpJson,
        HttpTransportDecision::Sse => TransportKind::HttpSse,
        HttpTransportDecision::WebSocket => TransportKind::WebSocket,
    }
}

pub fn parse_capped_json(bytes: &[u8], caps: &ResponseCaps) -> Result<Value, UpstreamError> {
    caps.enforce(CapScope::HttpJson, bytes.len())?;
    serde_json::from_slice(bytes).map_err(|_| UpstreamError::Unsupported {
        upstream: "http".to_owned(),
        capability: "http-json-parse",
    })
}

pub fn capped_sse_event<'a>(event: &'a str, caps: &ResponseCaps) -> Result<&'a str, UpstreamError> {
    caps.enforce(CapScope::HttpSseEvent, event.len())?;
    Ok(event)
}

#[cfg(test)]
#[path = "http_client_tests.rs"]
mod tests;
