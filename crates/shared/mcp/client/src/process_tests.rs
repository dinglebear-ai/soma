use super::*;

#[test]
fn cleanup_policy_is_explicit() {
    #[cfg(unix)]
    assert_eq!(
        default_cleanup_policy(),
        ChildCleanupPolicy::KillProcessGroup
    );
    #[cfg(windows)]
    assert_eq!(default_cleanup_policy(), ChildCleanupPolicy::KillWindowsJob);
}
