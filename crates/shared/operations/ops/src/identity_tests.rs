use super::*;

#[test]
fn generated_ids_are_uuid_v7_and_round_trip() {
    let id = OperationId::new();
    let parsed = OperationId::parse(id.as_str()).expect("generated id parses");
    assert_eq!(parsed, id);
    assert_eq!(Uuid::parse_str(id.as_str()).unwrap().get_version_num(), 7);
}

#[test]
fn uuid_ids_normalize_input() {
    let parsed = EventId::parse("01890F5E-7BBD-7CC3-98C8-BDC42B5F35BD").unwrap();
    assert_eq!(parsed.as_str(), "01890f5e-7bbd-7cc3-98c8-bdc42b5f35bd");
}

#[test]
fn invalid_uuid_is_rejected() {
    assert!(matches!(
        CorrelationId::parse("not-a-uuid"),
        Err(IdentityError::InvalidUuid {
            kind: "correlation",
            ..
        })
    ));
}

#[test]
fn references_reject_control_characters() {
    assert!(
        ActorRef::new(
            "soma", "bad
id"
        )
        .is_err()
    );
    assert!(ProducerRef::new("", "1.0.0").is_err());
}

#[test]
fn trace_context_is_bounded() {
    assert!(TraceContext::new(Some("00-abc"), None::<String>).is_ok());
    assert!(TraceContext::new(Some("x".repeat(513)), None::<String>).is_err());
}

#[test]
fn timestamp_preserves_explicit_milliseconds() {
    let timestamp = Timestamp::from_unix_millis(42);
    assert_eq!(timestamp.unix_millis(), 42);
}
