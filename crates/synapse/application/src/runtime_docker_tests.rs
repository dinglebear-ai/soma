use super::*;

#[test]
fn docker_state_mapping_is_closed_but_preserves_unknown_values() {
    assert_eq!(docker_state(Some("running")), Some(ContainerState::Running));
    assert_eq!(
        docker_state(Some("paused-by-runtime")),
        Some(ContainerState::Unknown("paused-by-runtime".into()))
    );
}
