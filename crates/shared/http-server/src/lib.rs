//! Reusable Axum server plumbing (plan section 3.12).
//!
//! `soma-http-api` (plan section 3.11) owns reusable *API surface* shapes —
//! response envelopes, error/problem-details bodies, probe DTOs, route
//! inventory metadata. This crate owns the layer below that: generic
//! *transport lifecycle* mechanics — listener binding, the Axum server run
//! loop, graceful shutdown, request-ID/tracing/timeout/body-limit
//! middleware, generic CORS configuration, generic health-check routing,
//! and a generic not-found/method-not-allowed rejection envelope.
//!
//! This crate must stay product-agnostic: no `/v1/*` Soma routes, no Soma
//! action names, no product auth policy, no Soma OpenAPI document content,
//! no embedded Soma UI assets, no action dispatch. Product routers compose
//! themselves out of these primitives; they do not belong here.

pub mod health;
pub mod middleware;
pub mod rejection;
pub mod server;
pub mod shutdown;

pub use server::{bind, serve, serve_with_shutdown, ServerError};
pub use shutdown::shutdown_signal;
