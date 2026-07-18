//! Reusable trace metadata extraction, integrating `rmcp-traces` for inbound
//! RMCP request `_meta`.
//!
//! This module owns the generic, protocol-level extraction step only. Turning
//! the recovered fields into a product-specific trace-context type (with its
//! own field names, propagation policy, and downstream plumbing) is a product
//! adapter concern and stays out of this crate.

use rmcp::model::Meta;

pub use rmcp_traces::{TraceSummary, TraceTrust};

/// Build a bounded [`TraceSummary`] from inbound RMCP request `_meta`.
///
/// This is a thin wrapper so MCP server implementations get discoverable,
/// documented trace extraction from `soma-mcp-server` without adding a direct
/// `rmcp-traces` dependency themselves.
pub fn trace_summary_from_meta(meta: &Meta, trust: TraceTrust) -> TraceSummary {
    TraceSummary::from_meta(meta, trust)
}

/// Raw W3C `traceparent`/`tracestate` header values recovered from `_meta`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawTraceFields {
    pub traceparent: Option<String>,
    pub tracestate: Option<String>,
}

/// Recover raw `traceparent`/`tracestate` values from `_meta`, gated on the
/// bounded [`TraceSummary`] confirming a validated trace id. Returns `None`
/// when no valid trace id was extracted (absent, malformed, or over budget).
pub fn raw_trace_fields_from_meta(meta: &Meta, trust: TraceTrust) -> Option<RawTraceFields> {
    let summary = trace_summary_from_meta(meta, trust);
    summary.trace_id_prefix()?;
    Some(RawTraceFields {
        traceparent: meta.get_traceparent().map(ToOwned::to_owned),
        tracestate: summary
            .has_tracestate()
            .then(|| meta.get_tracestate().map(ToOwned::to_owned))
            .flatten(),
    })
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
