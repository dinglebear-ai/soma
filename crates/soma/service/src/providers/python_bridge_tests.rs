use super::PYTHON_BRIDGE;

#[test]
fn embedded_bridge_contains_catalog_and_call_modes() {
    assert!(PYTHON_BRIDGE.contains("mode == \"catalog\""));
    assert!(PYTHON_BRIDGE.contains("mode == \"call\""));
    assert!(PYTHON_BRIDGE.contains("restrict_environment"));
}
