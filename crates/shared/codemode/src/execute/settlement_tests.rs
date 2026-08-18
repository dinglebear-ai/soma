use std::time::Duration;

use super::settlement::{
    RUNNER_RESULT_ACK_RESERVE, RUNNER_SETTLEMENT_GRACE, SettlementWatch, external_tool_deadline,
};

#[test]
fn settlement_watch_uses_full_grace_when_budget_allows() {
    let now = tokio::time::Instant::now();
    let execution_deadline = now + Duration::from_secs(10);
    let watch = SettlementWatch::new(now, execution_deadline);
    assert_eq!(watch.deadline, now + RUNNER_SETTLEMENT_GRACE);
    assert!(watch.grace_limited);
}

#[test]
fn settlement_watch_preserves_outer_deadline_at_equality() {
    let now = tokio::time::Instant::now();
    let execution_deadline = now + RUNNER_SETTLEMENT_GRACE;
    let watch = SettlementWatch::new(now, execution_deadline);
    assert_eq!(watch.deadline, execution_deadline);
    assert!(!watch.grace_limited);
}

#[test]
fn external_tool_deadline_reserves_result_acknowledgement_budget() {
    let now = tokio::time::Instant::now();
    let execution_deadline = now + Duration::from_secs(30);
    assert_eq!(
        external_tool_deadline(now, execution_deadline),
        execution_deadline - RUNNER_RESULT_ACK_RESERVE
    );
}

#[test]
fn external_tool_deadline_keeps_short_budget_intact() {
    let now = tokio::time::Instant::now();
    let execution_deadline = now + Duration::from_millis(400);
    assert_eq!(
        external_tool_deadline(now, execution_deadline),
        execution_deadline
    );
}
