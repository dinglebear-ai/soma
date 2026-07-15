use super::state::path::state_root;

#[test]
fn state_root_uses_soma_home() {
    assert!(state_root().ends_with(".soma-state"));
}
