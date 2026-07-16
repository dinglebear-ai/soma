use serde_json::json;

use super::discovery::generate_discovery_js;

#[test]
fn discovery_js_embeds_catalog() {
    let js = generate_discovery_js(&[json!({"id": "demo::call"})], 0.5).unwrap();
    assert!(js.contains("__codemodeDiscovery"));
    assert!(js.contains("demo::call"));
}
