//! Bounded, log-safe helpers for RMCP trace metadata.
//!
//! `rmcp-traces` complements RMCP's own `_meta` serialization. It does not own
//! MCP wire encoding and it does not attach result `_meta` in v1.
//!
//! ## RMCP Version
//!
//! This crate targets `rmcp 2.2.0`.
//!
//! ## Deferred V1 Surfaces
//!
//! Result `_meta` helpers are deferred because protocol-level metadata must be
//! budgeted together with Soma's normal, paged, cached-page, structured-error,
//! auth-denial, and protocol-denial paths.
//!
//! The optional `http` feature extracts inbound W3C `traceparent`,
//! `tracestate`, and, when explicitly enabled, `baggage` headers into RMCP
//! request metadata. Baggage is default-off. HTTP extraction never adds
//! outbound propagation or result `_meta`.
//! `tracestate` validation intentionally keeps Soma's stricter local policy
//! for now: empty or whitespace-only list members are rejected.
//!
//! Outbound HTTP propagation is deferred because baggage and sampled flags need
//! an application trust-boundary policy before public header forwarding is safe.

#![forbid(unsafe_code)]

mod trace_context;

#[cfg(feature = "http")]
pub mod http;

pub use trace_context::{
    TraceLimits, TraceParseError, TraceSummary, TraceTrust, BAGGAGE_KEY, TRACEPARENT_KEY,
    TRACESTATE_KEY,
};
