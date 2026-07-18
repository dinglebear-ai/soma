//! Generic "parse a JSON body, or fall back to a default when the client
//! sent none" helper — the shape every REST handler in this workspace that
//! accepts an optional body already needs.

use axum::{extract::rejection::JsonRejection, response::Response, Json};
use serde::de::DeserializeOwned;

use crate::response::json_rejection_response;

/// Outcome of extracting an optional JSON body: either the parsed (or
/// defaulted) value, or a response to return immediately because extraction
/// failed for a reason other than "no body was sent".
pub enum JsonBodyOutcome<T> {
    Params(T),
    Response(Response),
}

/// Parse `body`, or call `default` when the client omitted the JSON content
/// type and `allow_missing` is set. Any other rejection short-circuits with
/// a rendered error response.
pub fn json_body_or_else<T>(
    body: Result<Json<T>, JsonRejection>,
    allow_missing: bool,
    default: impl FnOnce() -> T,
) -> JsonBodyOutcome<T>
where
    T: DeserializeOwned,
{
    match body {
        Ok(Json(value)) => JsonBodyOutcome::Params(value),
        Err(JsonRejection::MissingJsonContentType(_)) if allow_missing => {
            JsonBodyOutcome::Params(default())
        }
        Err(error) => JsonBodyOutcome::Response(json_rejection_response(error)),
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        extract::FromRequest,
        http::{Request, StatusCode},
    };
    use serde_json::Value;

    use super::*;

    /// Drive a real extraction failure through Axum's own `FromRequest`
    /// machinery rather than constructing `JsonRejection` variants by hand —
    /// `MissingJsonContentType` is `#[non_exhaustive]` and not constructible
    /// outside axum-core.
    async fn extract_json(request: Request<Body>) -> Result<Json<Value>, JsonRejection> {
        Json::<Value>::from_request(request, &()).await
    }

    #[tokio::test]
    async fn missing_body_uses_default_when_allowed() {
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .body(Body::empty())
            .unwrap();
        let body = extract_json(request).await;
        match json_body_or_else(body, true, || serde_json::json!({})) {
            JsonBodyOutcome::Params(value) => assert_eq!(value, serde_json::json!({})),
            JsonBodyOutcome::Response(_) => panic!("expected defaulted params"),
        }
    }

    #[tokio::test]
    async fn missing_body_is_rejected_when_not_allowed() {
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .body(Body::empty())
            .unwrap();
        let body = extract_json(request).await;
        match json_body_or_else(body, false, || serde_json::json!({})) {
            JsonBodyOutcome::Response(response) => {
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            }
            JsonBodyOutcome::Params(_) => panic!("expected a rejection response"),
        }
    }

    #[tokio::test]
    async fn parsed_body_passes_through() {
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"a":1}"#))
            .unwrap();
        let body = extract_json(request).await;
        match json_body_or_else(body, true, || serde_json::json!({})) {
            JsonBodyOutcome::Params(value) => assert_eq!(value, serde_json::json!({"a": 1})),
            JsonBodyOutcome::Response(_) => panic!("expected parsed params"),
        }
    }
}
