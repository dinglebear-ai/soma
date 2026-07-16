use serde_json::json;

use crate::types::{CodeModeCatalogKind, ToolDescriptor};

use super::proxy::build_proxy;

#[test]
fn proxy_exposes_discovery_and_helpers() {
    let descriptor = ToolDescriptor {
        kind: CodeModeCatalogKind::Tool,
        id: "demo::ping".to_string(),
        name: "ping".to_string(),
        namespace: "demo".to_string(),
        description: "Ping".to_string(),
        schema: Some(json!({"type": "object"})),
        output_schema: None,
        signature: "codemode.demo.ping(params?)".to_string(),
        dts: "declare const ping: Function;".to_string(),
        tags: Vec::new(),
        inputs: Vec::new(),
    };
    let proxy = build_proxy(&[descriptor], 0.5).unwrap();
    assert!(proxy.contains("codemode.search"));
    assert!(proxy.contains("codemode.describe"));
    assert!(proxy.contains("codemode.demo.ping"));
    assert!(proxy.contains("codemode.state.readFile"));
}

#[cfg(feature = "openapi")]
#[test]
fn proxy_exposes_openapi_helper_when_feature_enabled() {
    let proxy = build_proxy(&[], 0.5).unwrap();
    assert!(proxy.contains("globalThis.openapi"));
    assert!(proxy.contains("openapi.call"));
}
