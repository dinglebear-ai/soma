#![cfg(feature = "http")]

use http::{HeaderMap, HeaderValue};
use rmcp_traces::http::{extract_http_trace, HttpTracePolicy};
use rmcp_traces::{TraceLimits, TraceTrust, BAGGAGE_KEY, TRACEPARENT_KEY, TRACESTATE_KEY};

const VALID_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";

#[test]
fn extracts_valid_trace_headers_and_strips_baggage_by_default() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.insert(TRACESTATE_KEY, "vendor=value".parse().unwrap());
    headers.insert(
        BAGGAGE_KEY,
        "sessionId=s123,region=us-east-1".parse().unwrap(),
    );

    let extraction = extract_http_trace(&headers, HttpTracePolicy::default());

    assert_eq!(extraction.meta.get_traceparent(), Some(VALID_TRACEPARENT));
    assert_eq!(extraction.meta.get_tracestate(), Some("vendor=value"));
    assert_eq!(extraction.meta.get_baggage(), None);
    assert_eq!(extraction.summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(extraction.summary.span_id_prefix(), Some("00f067aa"));
    assert_eq!(extraction.summary.sampled(), Some(true));
    assert_eq!(extraction.summary.trust(), TraceTrust::Untrusted);
    assert!(extraction.summary.has_tracestate());
    assert_eq!(extraction.summary.baggage_member_count(), 0);
    assert_eq!(extraction.summary.invalid_count(), 0);
}

#[test]
fn missing_traceparent_returns_empty_meta_with_configured_trust() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACESTATE_KEY, "vendor=value".parse().unwrap());
    headers.insert(BAGGAGE_KEY, "sessionId=s123".parse().unwrap());
    let policy = HttpTracePolicy {
        trust: TraceTrust::Trusted,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert!(extraction.meta.is_empty());
    assert_eq!(extraction.summary.trust(), TraceTrust::Trusted);
    assert_eq!(extraction.summary.trace_id_prefix(), None);
    assert!(!extraction.summary.has_tracestate());
    assert_eq!(extraction.summary.baggage_member_count(), 0);
    assert_eq!(extraction.summary.invalid_count(), 0);
}

#[test]
fn invalid_traceparent_suppresses_optional_metadata_counts() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, "00-invalid".parse().unwrap());
    headers.insert(TRACESTATE_KEY, "vendor=value".parse().unwrap());
    headers.insert(BAGGAGE_KEY, "sessionId=s123".parse().unwrap());
    let policy = HttpTracePolicy {
        include_baggage: true,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert!(extraction.meta.is_empty());
    assert_eq!(extraction.summary.trace_id_prefix(), None);
    assert!(!extraction.summary.has_tracestate());
    assert_eq!(extraction.summary.baggage_member_count(), 0);
    assert_eq!(extraction.summary.invalid_count(), 1);
    assert!(extraction.summary.invalid_reasons()[0].starts_with("traceparent"));
}

#[test]
fn duplicate_traceparent_is_rejected_without_optional_metadata() {
    let mut headers = HeaderMap::new();
    headers.append(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.append(
        TRACEPARENT_KEY,
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
            .parse()
            .unwrap(),
    );
    headers.insert(BAGGAGE_KEY, "sessionId=s123".parse().unwrap());
    let policy = HttpTracePolicy {
        include_baggage: true,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert!(extraction.meta.is_empty());
    assert_eq!(extraction.summary.trace_id_prefix(), None);
    assert_eq!(extraction.summary.baggage_member_count(), 0);
    assert_eq!(extraction.summary.invalid_count(), 1);
    assert_eq!(
        extraction.summary.invalid_reasons(),
        &["traceparent had multiple header values".to_owned()]
    );
}

#[test]
fn split_optional_headers_join_within_limits() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.append(TRACESTATE_KEY, "vendor=value".parse().unwrap());
    headers.append(TRACESTATE_KEY, "other=two".parse().unwrap());
    headers.append(BAGGAGE_KEY, "sessionId=s123".parse().unwrap());
    headers.append(BAGGAGE_KEY, "region=us-east-1".parse().unwrap());
    let policy = HttpTracePolicy {
        include_baggage: true,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert_eq!(
        extraction.meta.get_tracestate(),
        Some("vendor=value,other=two")
    );
    assert_eq!(
        extraction.meta.get_baggage(),
        Some("sessionId=s123,region=us-east-1")
    );
    assert!(extraction.summary.has_tracestate());
    assert_eq!(extraction.summary.baggage_member_count(), 2);
    assert_eq!(extraction.summary.sensitive_baggage_member_count(), 1);
    assert_eq!(extraction.summary.invalid_count(), 0);
}

#[test]
fn split_optional_headers_fail_before_over_allocation() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.append(TRACESTATE_KEY, "vendor=value".parse().unwrap());
    headers.append(TRACESTATE_KEY, "other=two".parse().unwrap());
    let policy = HttpTracePolicy {
        limits: TraceLimits {
            max_tracestate_len: 12,
            ..TraceLimits::default()
        },
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert_eq!(extraction.summary.trace_id_prefix(), Some("0af76519"));
    assert!(extraction.meta.get_tracestate().is_none());
    assert!(extraction.summary.invalid_reasons()[0].starts_with("tracestate exceeded 12 bytes"));
}

#[test]
fn split_baggage_headers_fail_before_over_allocation() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.append(BAGGAGE_KEY, "sessionId=s123".parse().unwrap());
    headers.append(BAGGAGE_KEY, "region=us-east-1".parse().unwrap());
    let policy = HttpTracePolicy {
        include_baggage: true,
        limits: TraceLimits {
            max_baggage_len: 13,
            ..TraceLimits::default()
        },
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert_eq!(extraction.summary.trace_id_prefix(), Some("0af76519"));
    assert!(extraction.meta.get_baggage().is_none());
    assert_eq!(extraction.summary.baggage_member_count(), 0);
    assert!(extraction.summary.invalid_reasons()[0].starts_with("baggage exceeded 13 bytes"));
}

#[test]
fn include_baggage_validates_and_counts_sensitive_members() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.insert(
        BAGGAGE_KEY,
        "email=alice@example.com,accessToken=super-secret-token,region=us-east-1"
            .parse()
            .unwrap(),
    );
    let policy = HttpTracePolicy {
        include_baggage: true,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert!(extraction.meta.get_baggage().is_some());
    assert_eq!(extraction.summary.baggage_member_count(), 3);
    assert_eq!(extraction.summary.sensitive_baggage_member_count(), 1);
    assert_eq!(extraction.summary.invalid_count(), 0);
}

#[test]
fn invalid_baggage_is_not_inserted_into_returned_meta() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.insert(BAGGAGE_KEY, "region=us-east-1;".parse().unwrap());
    let policy = HttpTracePolicy {
        include_baggage: true,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);

    assert!(extraction.meta.get_baggage().is_none());
    assert_eq!(extraction.summary.baggage_member_count(), 0);
    assert_eq!(extraction.summary.invalid_count(), 1);
    assert!(extraction.summary.invalid_reasons()[0].starts_with("baggage"));
}

#[test]
fn extraction_debug_is_redacted() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.insert(TRACESTATE_KEY, "vendor=secret-state".parse().unwrap());
    headers.insert(
        BAGGAGE_KEY,
        "email=alice@example.com,accessToken=super-secret-token,x-api-key=abc123,sessionId=s123"
            .parse()
            .unwrap(),
    );
    let policy = HttpTracePolicy {
        include_baggage: true,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);
    let debug = format!("{extraction:?}");

    assert!(debug.contains("trace_id_prefix"));
    assert!(!debug.contains(VALID_TRACEPARENT));
    assert!(!debug.contains("secret-state"));
    assert!(!debug.contains("alice@example.com"));
    assert!(!debug.contains("super-secret-token"));
    assert!(!debug.contains("abc123"));
    assert!(!debug.contains("s123"));
}

#[test]
fn trace_flags_with_reserved_bits_are_valid() {
    let mut headers = HeaderMap::new();
    headers.insert(
        TRACEPARENT_KEY,
        "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-03"
            .parse()
            .unwrap(),
    );

    let extraction = extract_http_trace(&headers, HttpTracePolicy::default());

    assert_eq!(extraction.summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(extraction.summary.sampled(), Some(true));
    assert_eq!(extraction.summary.invalid_count(), 0);
}

#[test]
fn non_visible_ascii_header_value_is_rejected_safely() {
    let mut headers = HeaderMap::new();
    headers.insert(
        TRACEPARENT_KEY,
        HeaderValue::from_bytes(b"00-\xff").unwrap(),
    );

    let extraction = extract_http_trace(&headers, HttpTracePolicy::default());

    assert!(extraction.meta.is_empty());
    assert_eq!(extraction.summary.invalid_count(), 1);
    assert_eq!(
        extraction.summary.invalid_reasons(),
        &["traceparent header value was not visible ASCII".to_owned()]
    );
    assert!(!format!("{extraction:?}").contains("\\xff"));
}

#[test]
fn non_visible_ascii_optional_headers_preserve_valid_traceparent() {
    let mut headers = HeaderMap::new();
    headers.insert(TRACEPARENT_KEY, VALID_TRACEPARENT.parse().unwrap());
    headers.insert(
        TRACESTATE_KEY,
        HeaderValue::from_bytes(b"vendor=\xff").unwrap(),
    );
    headers.insert(
        BAGGAGE_KEY,
        HeaderValue::from_bytes(b"region=\xff").unwrap(),
    );
    let policy = HttpTracePolicy {
        include_baggage: true,
        ..HttpTracePolicy::default()
    };

    let extraction = extract_http_trace(&headers, policy);
    let debug = format!("{extraction:?}");

    assert_eq!(extraction.summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(extraction.meta.get_traceparent(), Some(VALID_TRACEPARENT));
    assert_eq!(extraction.meta.get_tracestate(), None);
    assert_eq!(extraction.meta.get_baggage(), None);
    assert!(!extraction.summary.has_tracestate());
    assert_eq!(extraction.summary.baggage_member_count(), 0);
    assert_eq!(extraction.summary.invalid_count(), 2);
    assert!(extraction
        .summary
        .invalid_reasons()
        .iter()
        .any(|reason| reason == "tracestate header value was not visible ASCII"));
    assert!(extraction
        .summary
        .invalid_reasons()
        .iter()
        .any(|reason| reason == "baggage header value was not visible ASCII"));
    assert!(!debug.contains("\\xff"));
}
