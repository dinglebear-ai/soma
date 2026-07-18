//! Reusable HTTP API surface mechanics (plan section 3.11).
//!
//! `soma-http-server` (a later slice, plan section 3.12) owns generic Axum
//! server lifecycle — listener binding, graceful shutdown, middleware. This
//! crate owns the layer above that: reusable *API surface* shapes — response
//! envelopes, error/problem-details bodies, liveness/readiness probe DTOs,
//! route inventory metadata, and pagination query DTOs.
//!
//! This crate must stay product-agnostic: no Soma action names, no `/v1/*`
//! route paths, no auth policy, no product service/runtime state. Product
//! routes and their concrete data live in `soma-api`.

pub mod json;
pub mod pagination;
pub mod probe;
pub mod problem;
pub mod response;
pub mod route_inventory;
