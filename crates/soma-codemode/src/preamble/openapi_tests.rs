use super::openapi::generate_openapi_provider_js;

#[test]
fn openapi_provider_js_uses_call_tool_without_credentials() {
    let js = generate_openapi_provider_js();
    assert!(js.contains("globalThis.openapi"));
    assert!(js.contains(r#"callTool("openapi::" + label + "." + operationId"#));
    for forbidden in ["authorization", "bearer", "token", "apiKey", "credential"] {
        assert!(
            !js.to_ascii_lowercase()
                .contains(&forbidden.to_ascii_lowercase()),
            "OpenAPI proxy JS must not embed {forbidden}"
        );
    }
}
