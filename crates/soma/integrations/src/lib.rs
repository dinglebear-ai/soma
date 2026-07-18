//! Product adapters connecting `soma-application`'s transport-neutral ports
//! to Soma's shared engines (plan section 3.20).
//!
//! This is intentionally an outer-layer crate: it is the one place allowed to
//! see both `soma-application` ports and concrete shared engines
//! (`soma-gateway`, `soma-codemode`, and — once wired — `soma-openapi`) in
//! the same dependency graph. `apps/soma` constructs these adapters; it must
//! not contain their implementation logic.
//!
//! Modules:
//!   [`gateway`]           — [`GatewayPort`](soma_application::GatewayPort) implementation over `soma-gateway`'s `GatewayManager`.
//!   [`gateway_auth`]      — bridges `soma-auth`'s upstream OAuth runtime into `soma-gateway`'s generic OAuth traits (`oauth` feature).
//!   [`auth`]              — Soma's product auth default mapping (env prefix, scopes, cookie name) (`auth` feature).
//!   [`codemode`]          — [`CodeModePort`](soma_application::CodeModePort) implementation over `soma-codemode`.
//!   [`protected_routes`]  — bearer-token auth, scope authorization, and gateway-subset dispatch for protected MCP routes (`protected-http` feature).
//!
//! Not yet implemented here (see `soma-architecture-refactor-plan-v3.md` PR 11
//! notes on the bead for this slice): `OpenApiPort` has no product adapter —
//! `soma_application::OpenApiExecuteRequest` has no spec/label field and no
//! `soma_openapi::registry::OpenApiRegistry` is constructed anywhere in the
//! runtime yet, so a real adapter would require inventing an unspecified wire
//! shape rather than moving or wiring existing, tested behavior.

pub mod gateway;

#[cfg(feature = "auth")]
pub mod auth;
#[cfg(feature = "oauth")]
pub mod gateway_auth;
#[cfg(feature = "protected-http")]
pub mod protected_routes;
#[cfg(feature = "protected-http")]
mod protected_routes_proxy;
#[cfg(all(test, feature = "protected-http"))]
mod test_support;

pub mod codemode;

pub use codemode::CodeModeApplicationPort;
pub use gateway::GatewayApplicationPort;
