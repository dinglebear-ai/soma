//! Maps Soma's typed [`TraceHeaderMode`] config onto `rmcp-traces`' request-side
//! trace extraction, for both RMCP `_meta` (always available) and, when
//! trusted, inbound HTTP headers (only under the `http` Cargo feature).
//!
//! `_meta` always wins over HTTP headers: when `_meta` carries any trace key,
//! HTTP header values are never parsed, joined, counted, or logged. Only safe
//! presence booleans are recorded.

use rmcp::model::Meta;
use rmcp_traces::{TraceSummary, TraceTrust, BAGGAGE_KEY, TRACEPARENT_KEY, TRACESTATE_KEY};
use soma_config::TraceHeaderMode;
use soma_domain::TraceContext;

#[cfg(test)]
#[path = "trace_resolution_tests.rs"]
mod tests;

pub(crate) struct TraceResolution {
    pub summary: TraceSummary,
    pub trace_context: Option<TraceContext>,
    pub http_trace_headers_present: bool,
}

impl TraceResolution {
    pub(crate) fn from_meta_only(meta: &Meta) -> Self {
        let extraction = soma_mcp_server::trace::extract_trace_meta(meta, TraceTrust::Untrusted);
        Self {
            trace_context: trace_context_from_raw_fields(extraction.raw_fields),
            summary: extraction.summary,
            http_trace_headers_present: false,
        }
    }
}

#[cfg(test)]
pub(crate) fn trace_context_from_meta(meta: &Meta) -> Option<TraceContext> {
    let extraction = soma_mcp_server::trace::extract_trace_meta(meta, TraceTrust::Untrusted);
    trace_context_from_raw_fields(extraction.raw_fields)
}

fn trace_context_from_raw_fields(
    fields: Option<soma_mcp_server::trace::RawTraceFields>,
) -> Option<TraceContext> {
    let fields = fields?;
    Some(TraceContext {
        traceparent: fields.traceparent,
        tracestate: fields.tracestate,
    })
}

pub(crate) fn meta_has_any_trace_key(meta: &Meta) -> bool {
    meta.get(TRACEPARENT_KEY).is_some()
        || meta.get(TRACESTATE_KEY).is_some()
        || meta.get(BAGGAGE_KEY).is_some()
}

pub(crate) fn resolve_trace_resolution(
    mode: TraceHeaderMode,
    meta: &Meta,
    headers: Option<&::http::HeaderMap>,
) -> TraceResolution {
    match mode {
        TraceHeaderMode::Off => TraceResolution::from_meta_only(meta),
        TraceHeaderMode::Trusted | TraceHeaderMode::TrustedWithBaggage => {
            resolve_trusted(mode, meta, headers)
        }
    }
}

#[cfg(feature = "http")]
fn resolve_trusted(
    mode: TraceHeaderMode,
    meta: &Meta,
    headers: Option<&::http::HeaderMap>,
) -> TraceResolution {
    if meta_has_any_trace_key(meta) {
        let mut resolution = TraceResolution::from_meta_only(meta);
        resolution.http_trace_headers_present = headers.is_some_and(headers_have_any_trace_key);
        return resolution;
    }
    let Some(headers) = headers else {
        return TraceResolution::from_meta_only(meta);
    };
    let policy = rmcp_traces::http::HttpTracePolicy {
        trust: TraceTrust::Trusted,
        limits: Default::default(),
        include_baggage: matches!(mode, TraceHeaderMode::TrustedWithBaggage),
    };
    let extraction = rmcp_traces::http::extract_http_trace(headers, policy);
    TraceResolution {
        trace_context: trace_context_from_http_extraction(&extraction),
        summary: extraction.summary,
        http_trace_headers_present: headers_have_any_trace_key(headers),
    }
}

#[cfg(feature = "http")]
fn headers_have_any_trace_key(headers: &::http::HeaderMap) -> bool {
    headers.contains_key(TRACEPARENT_KEY)
        || headers.contains_key(TRACESTATE_KEY)
        || headers.contains_key(BAGGAGE_KEY)
}

#[cfg(feature = "http")]
fn trace_context_from_http_extraction(
    extraction: &rmcp_traces::http::HttpTraceExtraction,
) -> Option<TraceContext> {
    extraction.summary.trace_id_prefix()?;
    Some(TraceContext {
        traceparent: extraction.meta.get_traceparent().map(ToOwned::to_owned),
        tracestate: extraction
            .summary
            .has_tracestate()
            .then(|| extraction.meta.get_tracestate().map(ToOwned::to_owned))
            .flatten(),
    })
}

#[cfg(not(feature = "http"))]
fn resolve_trusted(
    _mode: TraceHeaderMode,
    meta: &Meta,
    _headers: Option<&::http::HeaderMap>,
) -> TraceResolution {
    TraceResolution::from_meta_only(meta)
}
