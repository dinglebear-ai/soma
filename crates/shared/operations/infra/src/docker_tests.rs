use super::*;

#[test]
fn container_states_are_closed_but_preserve_unknown_values() {
    assert_eq!(
        ContainerState::from_text(Some("running")),
        ContainerState::Running
    );
    assert_eq!(
        ContainerState::from_text(Some("new-state")),
        ContainerState::Unknown("new-state".into())
    );
    assert_eq!(
        ContainerState::from_text(None),
        ContainerState::Unknown(String::new())
    );
}

#[test]
fn list_defaults_are_read_only_and_complete() {
    assert_eq!(
        ContainerListOptions::default(),
        ContainerListOptions {
            all: true,
            state: None,
            label: None
        }
    );
    assert_eq!(
        ImageListOptions::default(),
        ImageListOptions {
            all: false,
            dangling_only: false
        }
    );
}
