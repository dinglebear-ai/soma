use super::*;

#[test]
fn activity_is_bounded_ordered_and_sanitized() {
    let log = ActivityLog::new(2);
    for index in 0..3 {
        log.record(
            "test",
            format!("op-{index}"),
            index == 2,
            Duration::ZERO,
            Some(
                "x
"
                .into(),
            ),
        );
    }
    let events = log.snapshot();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].operation, "op-1");
    assert_eq!(events[1].operation, "op-2");
    assert_eq!(events[1].message.as_deref(), Some("x"));
}
