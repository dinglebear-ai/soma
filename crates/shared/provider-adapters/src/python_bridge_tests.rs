use super::{PYTHON_BRIDGE, PYTHON_SDK, python_bridge_program};

#[test]
fn embedded_bridge_contains_versioned_catalog_and_call_modes() {
    assert!(PYTHON_BRIDGE.contains("mode == \"catalog\""));
    assert!(PYTHON_BRIDGE.contains("mode == \"call\""));
    assert!(PYTHON_BRIDGE.contains("request_identity(payload)"));
    assert!(PYTHON_BRIDGE.contains("schema_version != 1"));
    assert!(PYTHON_BRIDGE.contains("\"request_id\": request_id"));
    assert!(PYTHON_BRIDGE.contains("\"catalog\": jsonable(result)"));
    assert!(PYTHON_BRIDGE.contains("\"output\": jsonable(result, strict=True)"));
    assert!(PYTHON_BRIDGE.contains("restrict_environment"));
}

#[test]
fn composed_bridge_registers_the_embedded_authoring_sdk() {
    assert!(PYTHON_SDK.contains("def tool("));
    let program = python_bridge_program();
    assert!(program.contains("ModuleType(\"soma_provider\")"));
    assert!(program.contains("_soma_sys.modules[\"soma_provider\"]"));
    assert!(program.contains("__soma_tool__"));
    assert!(program.contains(PYTHON_BRIDGE));
}
