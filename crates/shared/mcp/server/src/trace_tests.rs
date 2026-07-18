use rmcp::model::Meta;

use super::*;

const VALID_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";

#[test]
fn raw_trace_fields_recovers_traceparent_and_tracestate() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");

    let fields = raw_trace_fields_from_meta(&meta, TraceTrust::Untrusted)
        .expect("valid traceparent should recover fields");
    assert_eq!(fields.traceparent.as_deref(), Some(VALID_TRACEPARENT));
    assert_eq!(fields.tracestate.as_deref(), Some("vendor=value"));
}

#[test]
fn raw_trace_fields_absent_without_valid_traceparent() {
    let mut meta = Meta::new();
    meta.set_traceparent("not-a-traceparent");

    assert!(raw_trace_fields_from_meta(&meta, TraceTrust::Untrusted).is_none());
}

#[test]
fn raw_trace_fields_drops_invalid_tracestate_but_keeps_traceparent() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("invalid tracestate");

    let fields = raw_trace_fields_from_meta(&meta, TraceTrust::Untrusted)
        .expect("valid traceparent should still recover fields");
    assert_eq!(fields.traceparent.as_deref(), Some(VALID_TRACEPARENT));
    assert_eq!(fields.tracestate, None);
}

#[test]
fn trace_summary_reports_prefixes_and_sample_flag() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);

    let summary = trace_summary_from_meta(&meta, TraceTrust::Untrusted);
    assert_eq!(summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(summary.span_id_prefix(), Some("00f067aa"));
    assert_eq!(summary.sampled(), Some(true));
}
