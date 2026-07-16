use super::runner_drive::local::is_local_provider;

#[test]
fn runner_drive_root_exposes_submodules() {
    assert!(is_local_provider("git::status"));
}
