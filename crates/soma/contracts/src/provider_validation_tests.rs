//! Smoke test for the deprecated facade: confirms
//! `soma_contracts::provider_validation` still resolves to the real
//! `soma_domain::provider_validation` implementation. Full behavioral
//! coverage lives in `soma-domain`.

use serde_json::json;

use super::*;

#[test]
fn facade_reexports_the_real_validator() {
    let manifest = json!({
        "schema_version": 1,
        "provider": { "name": "demo", "kind": "static-rust" },
        "tools": [],
    });
    validate_provider_manifest_value(&manifest).expect("minimal manifest should validate");
}
