use std::time::Duration;

/// Once a ToolResult/ToolError has been delivered, the sandbox has no external
/// I/O left to justify consuming a long execution timeout merely to flush
/// promise/microtask completion. Bound that final acknowledgement phase.
pub(super) const RUNNER_SETTLEMENT_GRACE: Duration = Duration::from_secs(5);
/// Reserve a small slice of the normal execution deadline for delivering a
/// ToolResult/ToolError and receiving the runner's Done/Error acknowledgement.
pub(super) const RUNNER_RESULT_ACK_RESERVE: Duration = Duration::from_millis(250);

#[derive(Clone, Copy)]
pub(super) struct SettlementWatch {
    pub(super) deadline: tokio::time::Instant,
    /// True only when the dedicated settlement grace is the actual limiter. If
    /// the outer execution deadline is earlier or equal, timeout classification
    /// must remain the ordinary Code Mode execution timeout.
    pub(super) grace_limited: bool,
}

impl SettlementWatch {
    pub(super) fn new(now: tokio::time::Instant, execution_deadline: tokio::time::Instant) -> Self {
        let grace_deadline = now + RUNNER_SETTLEMENT_GRACE;
        Self {
            deadline: grace_deadline.min(execution_deadline),
            grace_limited: grace_deadline < execution_deadline,
        }
    }
}

/// Deadline exposed to an external tool call. Reserve enough of the enclosing
/// execution budget for the host to relay ToolResult/ToolError and receive the
/// runner's terminal acknowledgement. Very short runs retain their full budget.
pub(super) fn external_tool_deadline(
    now: tokio::time::Instant,
    execution_deadline: tokio::time::Instant,
) -> tokio::time::Instant {
    let remaining = execution_deadline
        .checked_duration_since(now)
        .unwrap_or_default();
    if remaining > RUNNER_RESULT_ACK_RESERVE.saturating_mul(2) {
        execution_deadline - RUNNER_RESULT_ACK_RESERVE
    } else {
        execution_deadline
    }
}
