use super::limits::*;

#[test]
fn drive_limits_are_bounded() {
    let max_internal = MAX_INTERNAL_CALLS;
    let max_pending = MAX_PENDING_TOOL_CALLS;
    assert!(max_internal <= max_pending);
}
