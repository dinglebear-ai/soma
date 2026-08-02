use super::*;

#[test]
fn legacy_tool_names_are_stable() {
    assert_eq!(LegacyTool::Flux.as_str(), "flux");
    assert_eq!(LegacyTool::Scout.as_str(), "scout");
    assert_eq!(LegacyTool::Both.as_str(), "both");
}

#[test]
fn binding_keys_include_tool_action_and_subaction() {
    assert_ne!(
        LegacyBindingKey::new(LegacyTool::Flux, "help", None),
        LegacyBindingKey::new(LegacyTool::Scout, "help", None)
    );
    assert_ne!(
        LegacyBindingKey::new(LegacyTool::Flux, "docker", Some("info")),
        LegacyBindingKey::new(LegacyTool::Flux, "docker", Some("df"))
    );
}
