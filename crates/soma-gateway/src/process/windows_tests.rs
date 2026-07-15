use super::*;

#[test]
fn windows_cleanup_defaults_to_kill_on_drop_without_labby_winjob() {
    assert!(WindowsJobCleanup::default().kill_on_drop);
    assert!(job_cleanup_required());
}
