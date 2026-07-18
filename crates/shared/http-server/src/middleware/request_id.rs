//! Request-ID middleware.
//!
//! Assigns a UUID to `x-request-id` on the way in (unless the caller already
//! supplied one) and copies it back onto the response on the way out.
//! Compose both layers with [`tower::ServiceBuilder`]: `set` outermost (so
//! it runs first on the way in) and `propagate` innermost (so it runs last
//! on the way out, after the response has been fully built, and so
//! survives handler and error-mapping layers placed between them):
//!
//! ```
//! use tower::ServiceBuilder;
//! use soma_http_server::middleware::request_id;
//!
//! let _middleware = ServiceBuilder::new()
//!     .layer(request_id::set_request_id_layer())
//!     .layer(request_id::propagate_request_id_layer());
//! ```
//!
//! Note: [`crate::middleware::tracing::trace_layer`] uses `tower_http`'s
//! *default* HTTP span, which does **not** automatically include the
//! request ID. A product wanting the ID inside its trace spans needs a
//! custom `tower_http::trace::TraceLayer::new_for_http().make_span_with(..)`
//! that reads the `x-request-id` header itself — layering `trace_layer()`
//! between `set`/`propagate` alone does not achieve that.

use axum::http::HeaderName;
pub use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};

/// Default request-ID header name shared across Soma HTTP surfaces.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

fn header_name() -> HeaderName {
    HeaderName::from_static(REQUEST_ID_HEADER)
}

/// Layer that assigns a random UUID request ID when the incoming request
/// does not already carry one.
pub fn set_request_id_layer() -> SetRequestIdLayer<MakeRequestUuid> {
    SetRequestIdLayer::new(header_name(), MakeRequestUuid)
}

/// Layer that copies the request-scoped ID from the request onto the
/// response headers.
pub fn propagate_request_id_layer() -> PropagateRequestIdLayer {
    PropagateRequestIdLayer::new(header_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::{ServiceBuilder, ServiceExt};

    // `set_request_id_layer` must be outermost (added first) so it assigns
    // the ID before anything downstream runs; `propagate_request_id_layer`
    // must be innermost (added last, closest to the app) so it captures
    // that ID off the request and copies it onto the response.

    #[tokio::test]
    async fn assigns_and_propagates_a_request_id_when_absent() {
        let app = Router::new().route("/", get(|| async { "ok" })).layer(
            ServiceBuilder::new()
                .layer(set_request_id_layer())
                .layer(propagate_request_id_layer()),
        );

        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        let id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("response should carry a request id");
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn preserves_an_incoming_request_id() {
        let app = Router::new().route("/", get(|| async { "ok" })).layer(
            ServiceBuilder::new()
                .layer(set_request_id_layer())
                .layer(propagate_request_id_layer()),
        );

        let request = Request::builder()
            .uri("/")
            .header(REQUEST_ID_HEADER, "caller-supplied-id")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.headers().get(REQUEST_ID_HEADER).unwrap(),
            "caller-supplied-id"
        );
    }
}
