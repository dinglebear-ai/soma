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

/// One bounded parse of request `_meta`, keeping the validated summary and raw
/// fields tied to the same input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceMetaExtraction {
    pub summary: TraceSummary,
    pub raw_fields: Option<RawTraceFields>,
}

/// Parse request `_meta` once and derive both safe summary and gated raw fields.
pub fn extract_trace_meta(meta: &Meta, trust: TraceTrust) -> TraceMetaExtraction {
    let summary = trace_summary_from_meta(meta, trust);
    let raw_fields = raw_trace_fields_from_summary(meta, &summary);
    TraceMetaExtraction {
        summary,
        raw_fields,
    }
}

/// Recover raw `traceparent`/`tracestate` values from `_meta`, gated on the
/// bounded [`TraceSummary`] confirming a validated trace id. Returns `None`
/// when no valid trace id was extracted (absent, malformed, or over budget).
pub fn raw_trace_fields_from_meta(meta: &Meta, trust: TraceTrust) -> Option<RawTraceFields> {
    extract_trace_meta(meta, trust).raw_fields
}

fn raw_trace_fields_from_summary(meta: &Meta, summary: &TraceSummary) -> Option<RawTraceFields> {
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
