use super::*;

#[test]
fn noop_usage_sink_accepts_events() {
    NoopUsageSink.record(UsageEvent {
        action: "gateway.test".to_owned(),
        upstream: None,
        success: true,
        bytes: 0,
    });
}

#[test]
fn memory_usage_sink_records_events() {
    let sink = MemoryUsageSink::shared();
    sink.record(UsageEvent {
        action: "call_tool".to_owned(),
        upstream: Some("mock".to_owned()),
        success: true,
        bytes: 7,
    });

    assert_eq!(sink.events()[0].bytes, 7);
}
