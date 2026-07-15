use super::quota::WorkspaceQuota;

#[test]
fn quota_checks_new_bytes_once() {
    let quota = WorkspaceQuota { max_bytes: 10 };
    assert!(quota.check(5, 5));
    assert!(!quota.check(5, 6));
}
