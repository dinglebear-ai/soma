//! Thin, private PyO3 bindings for deterministic Soma provider semantics.
//!
//! Reusable behavior remains in PyO3-free Rust crates. This extension only
//! translates Python inputs and errors at the package boundary.

use pyo3::{exceptions::PyValueError, prelude::*};
use serde_json::Value;
use soma_provider_core::validate_provider_manifest_value;

const PROVIDER_SCHEMA_VERSION: u32 = 1;

#[pyfunction]
fn sdk_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction]
const fn provider_schema_version() -> u32 {
    PROVIDER_SCHEMA_VERSION
}

#[pyfunction]
fn validate_manifest_json(document: &str) -> PyResult<String> {
    let value: Value = serde_json::from_str(document)
        .map_err(|error| PyValueError::new_err(format!("invalid provider JSON: {error}")))?;
    let catalog = validate_provider_manifest_value(&value)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    serde_json::to_string(&catalog)
        .map_err(|error| PyValueError::new_err(format!("provider serialization failed: {error}")))
}

#[pymodule]
fn _soma_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(sdk_version, module)?)?;
    module.add_function(wrap_pyfunction!(provider_schema_version, module)?)?;
    module.add_function(wrap_pyfunction!(validate_manifest_json, module)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MANIFEST: &str = r#"{
        "schema_version": 1,
        "provider": {"name": "native-test", "kind": "static-rust"},
        "tools": [{
            "name": "native_echo",
            "description": "Validate through provider-core.",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }
        }]
    }"#;

    #[test]
    fn native_versions_match_package_contract() {
        assert_eq!(sdk_version(), "0.2.0");
        assert_eq!(provider_schema_version(), 1);
    }

    #[test]
    fn native_validation_uses_provider_core_semantics() {
        Python::initialize();
        let normalized = validate_manifest_json(VALID_MANIFEST).expect("valid manifest");
        let value: Value = serde_json::from_str(&normalized).expect("normalized JSON");
        assert_eq!(value["provider"]["name"], "native-test");

        let error = validate_manifest_json("not JSON").expect_err("invalid JSON");
        assert!(error.to_string().contains("invalid provider JSON"));
    }
}
