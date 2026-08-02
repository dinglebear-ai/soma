use super::*;

fn event() -> ProgressEvent {
    ProgressEvent::new(
        OperationId::new(),
        OperationName::new("image.pull").unwrap(),
        1,
        Timestamp::from_unix_millis(10),
        "downloading",
    )
    .unwrap()
}

#[test]
fn unknown_total_is_explicit() {
    let event = event().with_amount(30, None, Some("pages")).unwrap();
    assert_eq!(event.current(), Some(30));
    assert_eq!(event.total(), None);
    assert_eq!(event.unit(), Some("pages"));
}

#[test]
fn current_cannot_exceed_total() {
    assert_eq!(
        event().with_amount(301, Some(300), Some("pages")),
        Err(ProgressError::InvalidAmount {
            current: 301,
            total: Some(300),
        })
    );
}

#[test]
fn zero_total_is_rejected() {
    assert!(event().with_amount(0, Some(0), None::<String>).is_err());
}

#[test]
fn messages_are_bounded() {
    assert!(event().with_message("fetching page 30 of 300").is_ok());
    assert!(event().with_message("x".repeat(1_025)).is_err());
}

#[test]
fn noop_sink_accepts_events() {
    NoopProgressSink.report(&event()).unwrap();
}
