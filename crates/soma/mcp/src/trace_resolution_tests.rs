use super::*;
use ::http::{HeaderMap, HeaderValue};
use rmcp::model::MetaObject;
use soma_config::TraceHeaderMode;

const VALID_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";
#[cfg(feature = "http")]
const OTHER_TRACEPARENT: &str = "00-11112222333344445555666677778888-1111222233334444-01";

fn headers_with(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        headers.insert(
            *name,
            HeaderValue::from_str(value).expect("valid header value"),
        );
    }
    headers
}

#[test]
fn off_mode_ignores_http_headers_even_when_present() {
    let meta = MetaObject::new();
    let headers = headers_with(&[("traceparent", VALID_TRACEPARENT)]);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Off, &meta, Some(&headers));

    assert!(resolution.summary.trace_id_prefix().is_none());
    assert!(!resolution.http_trace_headers_present);
    assert!(resolution.trace_context.is_none());
}

#[test]
fn off_mode_still_summarizes_meta_traceparent() {
    let mut meta = MetaObject::new();
    meta.set_traceparent(VALID_TRACEPARENT);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Off, &meta, None);

    assert_eq!(resolution.summary.trace_id_prefix(), Some("0af76519"));
    assert!(!resolution.http_trace_headers_present);
}

#[test]
fn meta_only_resolution_derives_context_from_its_validated_summary() {
    let mut meta = MetaObject::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");

    let resolution = TraceResolution::from_meta_only(&meta);

    assert_eq!(resolution.summary.trace_id_prefix(), Some("0af76519"));
    assert!(resolution.summary.has_tracestate());
    assert_eq!(
        resolution.trace_context,
        Some(TraceContext {
            traceparent: Some(VALID_TRACEPARENT.to_owned()),
            tracestate: Some("vendor=value".to_owned()),
        })
    );
}

#[cfg(feature = "http")]
#[test]
fn trusted_mode_extracts_traceparent_and_tracestate_from_headers() {
    let meta = MetaObject::new();
    let headers = headers_with(&[
        ("traceparent", VALID_TRACEPARENT),
        ("tracestate", "vendor=value"),
    ]);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, Some(&headers));

    assert_eq!(resolution.summary.trace_id_prefix(), Some("0af76519"));
    assert!(resolution.summary.has_tracestate());
    assert!(resolution.http_trace_headers_present);
    let trace_context = resolution
        .trace_context
        .expect("trace context should be set");
    assert_eq!(
        trace_context.traceparent.as_deref(),
        Some(VALID_TRACEPARENT)
    );
    assert_eq!(trace_context.tracestate.as_deref(), Some("vendor=value"));
}

#[cfg(feature = "http")]
#[test]
fn trusted_mode_strips_baggage_even_when_present() {
    let meta = MetaObject::new();
    let headers = headers_with(&[
        ("traceparent", VALID_TRACEPARENT),
        ("baggage", "region=us-east-1"),
    ]);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, Some(&headers));

    assert_eq!(resolution.summary.baggage_member_count(), 0);
}

#[cfg(feature = "http")]
#[test]
fn trusted_with_baggage_mode_summarizes_baggage_safely() {
    let meta = MetaObject::new();
    let headers = headers_with(&[
        ("traceparent", VALID_TRACEPARENT),
        ("baggage", "region=us-east-1,accessToken=super-secret-token"),
    ]);

    let resolution =
        resolve_trace_resolution(TraceHeaderMode::TrustedWithBaggage, &meta, Some(&headers));

    assert_eq!(resolution.summary.baggage_member_count(), 2);
    assert_eq!(resolution.summary.sensitive_baggage_member_count(), 1);
}

#[test]
fn trusted_mode_with_no_headers_falls_back_to_meta_only() {
    let meta = MetaObject::new();
    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, None);
    assert!(resolution.summary.trace_id_prefix().is_none());
    assert!(!resolution.http_trace_headers_present);
}

#[test]
fn trusted_mode_ignores_tracestate_without_a_valid_traceparent() {
    let meta = MetaObject::new();
    let headers = headers_with(&[("tracestate", "vendor=value")]);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, Some(&headers));

    assert!(!resolution.summary.has_tracestate());
    assert!(resolution.trace_context.is_none());
}

#[cfg(feature = "http")]
#[test]
fn trusted_mode_records_invalid_reason_for_malformed_traceparent() {
    let meta = MetaObject::new();
    let headers = headers_with(&[("traceparent", "not-a-valid-traceparent")]);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, Some(&headers));

    assert!(resolution.summary.trace_id_prefix().is_none());
    assert!(resolution.summary.invalid_count() > 0);
    assert!(resolution.trace_context.is_none());
}

#[cfg(feature = "http")]
#[test]
fn meta_traceparent_wins_and_http_extraction_is_skipped() {
    let mut meta = MetaObject::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    let headers = headers_with(&[("traceparent", OTHER_TRACEPARENT)]);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, Some(&headers));

    assert_eq!(resolution.summary.trace_id_prefix(), Some("0af76519"));
    assert!(resolution.http_trace_headers_present);
    assert!(meta_has_any_trace_key(&meta));
}

#[cfg(feature = "http")]
#[test]
fn meta_baggage_key_alone_triggers_conflict_detection_without_traceparent() {
    let mut meta = MetaObject::new();
    meta.set_baggage("region=us-east-1");
    let headers = headers_with(&[
        ("traceparent", VALID_TRACEPARENT),
        ("baggage", "http-one=value,http-two=value"),
    ]);

    let resolution =
        resolve_trace_resolution(TraceHeaderMode::TrustedWithBaggage, &meta, Some(&headers));

    assert!(resolution.http_trace_headers_present);
    assert!(meta_has_any_trace_key(&meta));
    assert_eq!(
        resolution.summary.baggage_member_count(),
        1,
        "only the authoritative _meta baggage member should be summarized"
    );
}

#[test]
fn meta_trace_key_present_but_no_headers_means_no_conflict() {
    let mut meta = MetaObject::new();
    meta.set_traceparent(VALID_TRACEPARENT);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, None);

    assert!(!resolution.http_trace_headers_present);
    assert!(meta_has_any_trace_key(&meta));
}

#[cfg(feature = "http")]
#[test]
fn ordinary_http_headers_do_not_report_trace_presence_or_conflict() {
    let mut meta = MetaObject::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    let headers = headers_with(&[("authorization", "Bearer ordinary-request-token")]);

    let resolution = resolve_trace_resolution(TraceHeaderMode::Trusted, &meta, Some(&headers));
    let trace_context_conflict =
        meta_has_any_trace_key(&meta) && resolution.http_trace_headers_present;

    assert!(!resolution.http_trace_headers_present);
    assert!(!trace_context_conflict);
    assert_eq!(resolution.summary.trace_id_prefix(), Some("0af76519"));
}

#[test]
fn meta_has_any_trace_key_detects_each_key_independently() {
    assert!(!meta_has_any_trace_key(&MetaObject::new()));

    let mut traceparent_only = MetaObject::new();
    traceparent_only.set_traceparent(VALID_TRACEPARENT);
    assert!(meta_has_any_trace_key(&traceparent_only));

    let mut tracestate_only = MetaObject::new();
    tracestate_only.set_tracestate("vendor=value");
    assert!(meta_has_any_trace_key(&tracestate_only));

    let mut baggage_only = MetaObject::new();
    baggage_only.set_baggage("region=us-east-1");
    assert!(meta_has_any_trace_key(&baggage_only));
}

#[test]
fn trace_context_from_meta_is_none_without_a_valid_traceparent() {
    let meta = MetaObject::new();
    assert!(trace_context_from_meta(&meta).is_none());
}

#[test]
fn trace_context_from_meta_returns_traceparent_and_tracestate() {
    let mut meta = MetaObject::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");

    let context = trace_context_from_meta(&meta).expect("trace context should be present");
    assert_eq!(context.traceparent.as_deref(), Some(VALID_TRACEPARENT));
    assert_eq!(context.tracestate.as_deref(), Some("vendor=value"));
}
