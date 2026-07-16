use rmcp::model::Meta;
use rmcp_traces::{TraceLimits, TraceSummary, TraceTrust};
use serde_json::json;

const VALID_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";

#[test]
fn summary_summarizes_valid_trace_metadata_without_raw_values() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");
    meta.set_baggage("region=us-east-1,accessToken=super-secret-token");

    let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);

    assert_eq!(summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(summary.span_id_prefix(), Some("00f067aa"));
    assert_eq!(summary.sampled(), Some(true));
    assert_eq!(summary.trust(), TraceTrust::Untrusted);
    assert!(summary.has_tracestate());
    assert_eq!(summary.baggage_member_count(), 2);
    assert_eq!(summary.sensitive_baggage_member_count(), 1);
    assert_eq!(summary.invalid_count(), 0);
    assert!(!format!("{summary:?}").contains("super-secret-token"));
}

#[test]
fn summary_preserves_valid_traceparent_when_optional_metadata_is_invalid() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_baggage("a=1,b=2,c=3");
    let limits = TraceLimits {
        max_baggage_members: 2,
        ..TraceLimits::default()
    };

    let summary = TraceSummary::from_meta_with_limits(&meta, TraceTrust::Untrusted, limits);

    assert_eq!(summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(summary.span_id_prefix(), Some("00f067aa"));
    assert_eq!(summary.sampled(), Some(true));
    assert_eq!(summary.invalid_count(), 1);
    assert_eq!(
        summary.invalid_reasons()[0],
        "baggage exceeded 2 members (actual at least 3)"
    );
    assert_eq!(summary.baggage_member_count(), 0);
    assert_eq!(summary.sensitive_baggage_member_count(), 0);
}

#[test]
fn summary_collects_multiple_safe_invalid_reasons() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");
    meta.set_baggage("a=1,b=2,c=3");
    let limits = TraceLimits {
        max_tracestate_len: 8,
        max_baggage_members: 2,
        ..TraceLimits::default()
    };

    let summary = TraceSummary::from_meta_with_limits(&meta, TraceTrust::Untrusted, limits);

    assert_eq!(summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(summary.invalid_count(), 2);
    assert!(summary
        .invalid_reasons()
        .iter()
        .any(|reason| reason == "tracestate exceeded 8 bytes (actual 12)"));
    assert!(summary
        .invalid_reasons()
        .iter()
        .any(|reason| reason == "baggage exceeded 2 members (actual at least 3)"));
    assert!(!format!("{summary:?}").contains("vendor=value"));
    assert!(!format!("{summary:?}").contains("a=1"));
}

#[test]
fn summary_requires_traceparent_before_accepting_tracestate() {
    let mut meta = Meta::new();
    meta.set_tracestate("vendor=value");
    meta.set_baggage("sessionId=s123,region=us-east-1");

    let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);

    assert_eq!(summary.trace_id_prefix(), None);
    assert_eq!(summary.span_id_prefix(), None);
    assert_eq!(summary.sampled(), None);
    assert_eq!(summary.trust(), TraceTrust::Untrusted);
    assert!(!summary.has_tracestate());
    assert_eq!(summary.baggage_member_count(), 2);
    assert_eq!(summary.sensitive_baggage_member_count(), 1);
    assert_eq!(summary.invalid_count(), 1);
    assert_eq!(
        summary.invalid_reasons()[0],
        "tracestate requires a valid traceparent"
    );
}

#[test]
fn summary_reports_invalid_optional_metadata_without_traceparent() {
    let mut meta = Meta::new();
    meta.insert("tracestate".to_owned(), json!(123));
    meta.set_baggage("a=1,b=2,c=3");
    let limits = TraceLimits {
        max_baggage_members: 2,
        ..TraceLimits::default()
    };

    let summary = TraceSummary::from_meta_with_limits(&meta, TraceTrust::Untrusted, limits);

    assert_eq!(summary.trace_id_prefix(), None);
    assert_eq!(summary.invalid_count(), 2);
    assert!(summary
        .invalid_reasons()
        .iter()
        .any(|reason| reason == "tracestate was not a string"));
    assert!(summary
        .invalid_reasons()
        .iter()
        .any(|reason| reason == "baggage exceeded 2 members (actual at least 3)"));
}

#[test]
fn summary_rejects_malformed_tracestate_safely() {
    for (tracestate, reason) in [
        (
            "vendor=value,vendor=other",
            "tracestate contained a duplicate key",
        ),
        ("Vendor=value", "tracestate format was invalid"),
        ("vendor", "tracestate format was invalid"),
        ("vendor=value,,other=two", "tracestate format was invalid"),
        (
            "vendor=value,   ,other=two",
            "tracestate format was invalid",
        ),
    ] {
        let mut meta = Meta::new();
        meta.set_traceparent(VALID_TRACEPARENT);
        meta.set_tracestate(tracestate);

        let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);

        assert!(!summary.has_tracestate());
        assert_eq!(summary.invalid_reasons(), &[reason.to_owned()]);
        assert!(!format!("{summary:?}").contains(tracestate));
    }

    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate(
        (0..33)
            .map(|i| format!("v{i}=x"))
            .collect::<Vec<_>>()
            .join(","),
    );

    let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);

    assert_eq!(
        summary.invalid_reasons(),
        &["tracestate exceeded 32 members (actual at least 33)".to_owned()]
    );
}

#[test]
fn summary_rejects_malformed_baggage_and_counts_bad_members_toward_limit() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_baggage("token");

    let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);

    assert_eq!(
        summary.invalid_reasons(),
        &["baggage member format was invalid".to_owned()]
    );
    assert_eq!(summary.baggage_member_count(), 0);
    assert_eq!(summary.sensitive_baggage_member_count(), 0);

    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_baggage("ok=1,token");
    let limits = TraceLimits {
        max_baggage_members: 1,
        ..TraceLimits::default()
    };

    let summary = TraceSummary::from_meta_with_limits(&meta, TraceTrust::Untrusted, limits);

    assert_eq!(
        summary.invalid_reasons(),
        &["baggage exceeded 1 members (actual at least 2)".to_owned()]
    );
}

#[test]
fn absent_or_non_string_trace_metadata_is_fail_soft_for_summaries() {
    let meta = Meta::new();
    assert_eq!(
        TraceSummary::from_meta(&meta, TraceTrust::Untrusted).invalid_count(),
        0
    );

    let mut meta = Meta::new();
    meta.insert("traceparent".to_owned(), json!(123));

    let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);
    assert_eq!(summary.invalid_count(), 1);
    assert_eq!(summary.invalid_reasons()[0], "traceparent was not a string");
}

#[test]
fn trace_flags_accept_reserved_bits_and_keep_sampled_bit() {
    let mut meta = Meta::new();
    meta.set_traceparent("00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-03");

    let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);

    assert_eq!(summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(summary.sampled(), Some(true));
    assert_eq!(summary.invalid_count(), 0);
}

#[test]
fn summary_never_contains_raw_baggage_values() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_baggage(
        "email=alice@example.com,accessToken=super-secret-token,x-api-key=abc123,sessionId=s123",
    );

    let summary = TraceSummary::from_meta(&meta, TraceTrust::Untrusted);
    let debug = format!("{summary:?}");

    assert_eq!(summary.baggage_member_count(), 4);
    assert_eq!(summary.sensitive_baggage_member_count(), 3);
    assert!(debug.contains("trace_id_prefix"));
    assert!(debug.contains("0af76519"));
    assert!(!debug.contains("alice@example.com"));
    assert!(!debug.contains("super-secret-token"));
    assert!(!debug.contains("abc123"));
    assert!(!debug.contains("s123"));
}
