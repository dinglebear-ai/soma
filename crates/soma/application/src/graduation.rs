//! Honest Python-to-Rust/component graduation workflow.
//!
//! The workflow scaffolds adapters and verifies recorded behavior. It never
//! claims to translate arbitrary Python business logic.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// One recorded Python input/output pair used for component conformance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraduationFixture {
    /// Stable fixture label.
    pub name: String,
    /// Canonical provider invocation envelope.
    pub input: Value,
    /// Result recorded from the source Python provider.
    pub expected: Value,
}

/// Durable state for one graduation workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraduationState {
    /// State-file schema version.
    pub schema_version: u32,
    /// Original Python source retained for audit and manual porting.
    pub source: PathBuf,
    /// Verified, immutable component waiting for activation.
    pub candidate: Option<PathBuf>,
    /// Currently active component artifact.
    pub active: Option<PathBuf>,
    /// Previously active component retained for rollback.
    pub previous: Option<PathBuf>,
}

/// Scaffold a reusable Rust core plus thin PyO3 and WIT adapter placeholders.
pub fn graduate(source: &Path, workspace: &Path, fixtures: Option<&Path>) -> anyhow::Result<Value> {
    if !source.is_file() {
        anyhow::bail!("graduation source does not exist: {}", source.display());
    }
    if workspace.exists() {
        anyhow::bail!(
            "graduation workspace already exists: {}",
            workspace.display()
        );
    }
    let parent = workspace
        .parent()
        .ok_or_else(|| anyhow::anyhow!("graduation workspace requires a parent"))?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".soma-graduate-{}", unique_suffix()));
    if staging.exists() {
        anyhow::bail!(
            "graduation staging path already exists: {}",
            staging.display()
        );
    }
    fs::create_dir_all(staging.join("src"))?;
    fs::create_dir_all(staging.join("fixtures"))?;
    fs::create_dir_all(staging.join("artifacts"))?;
    fs::copy(source, staging.join("source.py"))?;
    let fixture_destination = staging.join("fixtures/conformance-v1.json");
    if let Some(fixtures) = fixtures {
        let corpus: Vec<GraduationFixture> = serde_json::from_slice(&fs::read(fixtures)?)?;
        if corpus.is_empty() {
            anyhow::bail!("graduation fixture corpus must not be empty");
        }
        fs::write(&fixture_destination, serde_json::to_vec_pretty(&corpus)?)?;
    } else {
        fs::write(&fixture_destination, b"[]\n")?;
    }
    fs::write(
        staging.join("fixtures/README.md"),
        "Record canonical Python invocation envelopes and expected JSON results in \
         `conformance-v1.json` before comparing or activating a component.\n",
    )?;
    fs::create_dir_all(staging.join("wit"))?;
    fs::write(
        staging.join("wit/world.wit"),
        include_str!("../../../../wit/soma-provider/world.wit"),
    )?;
    fs::write(
        staging.join("Cargo.toml"),
        r#"[package]
name = "graduated-soma-provider"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[features]
default = []
component = ["dep:wit-bindgen"]
python = ["dep:pyo3"]

[dependencies]
serde_json = "1"
pyo3 = { version = "0.28", optional = true, features = ["abi3-py311", "extension-module"] }
wit-bindgen = { version = "0.57.1", optional = true }
"#,
    )?;
    fs::write(
        staging.join("src/core.rs"),
        r#"pub fn invoke(_input: serde_json::Value) -> Result<serde_json::Value, String> {
    Err("manual rewrite required: port the provider business logic into this Rust core".to_owned())
}
"#,
    )?;
    fs::write(
        staging.join("src/lib.rs"),
        "mod core;\npub use core::invoke;\n\
         #[cfg(feature = \"component\")]\nmod component;\n\
         #[cfg(feature = \"python\")]\nmod python;\n",
    )?;
    fs::write(
        staging.join("src/component.rs"),
        r#"wit_bindgen::generate!({
    path: "wit",
    world: "provider",
});

struct ComponentProvider;

impl Guest for ComponentProvider {
    fn invoke(input_json: String) -> Result<String, String> {
        let input = serde_json::from_str(&input_json).map_err(|error| error.to_string())?;
        let output = crate::core::invoke(input)?;
        serde_json::to_string(&output).map_err(|error| error.to_string())
    }
}

export!(ComponentProvider);
"#,
    )?;
    fs::write(
        staging.join("src/python.rs"),
        r#"use pyo3::{prelude::*, types::PyModule};

#[pyfunction]
fn invoke(input_json: &str) -> PyResult<String> {
    let input = serde_json::from_str(input_json)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
    let output = crate::core::invoke(input)
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
    serde_json::to_string(&output)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
}

#[pymodule]
fn _soma_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(invoke, module)?)?;
    Ok(())
}
"#,
    )?;
    write_state(
        &staging,
        &GraduationState {
            schema_version: 1,
            source: source.to_path_buf(),
            candidate: None,
            active: None,
            previous: None,
        },
    )?;
    fs::rename(&staging, workspace)?;
    Ok(json!({
        "ok": true,
        "workspace": workspace,
        "manual_rewrite_required": true,
        "translated_business_logic": false,
        "fixtures_imported": fixtures.is_some(),
    }))
}

/// Build (or import), verify, and publish an immutable candidate artifact.
pub fn build_component(workspace: &Path, component: Option<&Path>) -> anyhow::Result<Value> {
    let built_component;
    let component = if let Some(component) = component {
        component
    } else {
        let status = Command::new("cargo")
            .args([
                "build",
                "--manifest-path",
                &workspace.join("Cargo.toml").to_string_lossy(),
                "--target",
                "wasm32-wasip2",
                "--features",
                "component",
            ])
            .status()?;
        if !status.success() {
            anyhow::bail!("graduated component build failed with status {status}");
        }
        built_component = workspace.join("target/wasm32-wasip2/debug/graduated_soma_provider.wasm");
        &built_component
    };
    soma_provider_adapters::wasm::verify_component_artifact(component)
        .map_err(anyhow::Error::msg)?;
    let bytes = fs::read(component)?;
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let destination = workspace
        .join("artifacts")
        .join(format!("candidate-{digest}.wasm"));
    fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| anyhow::anyhow!("candidate path requires a parent"))?,
    )?;
    if !destination.exists() {
        let staging = destination.with_extension(format!("staging-{}", unique_suffix()));
        fs::write(&staging, bytes)?;
        fs::rename(staging, &destination)?;
    }
    let mut state = read_state(workspace)?;
    state.candidate = Some(destination.clone());
    write_state(workspace, &state)?;
    Ok(json!({"ok": true, "candidate": destination, "sha256": digest}))
}

/// Validate a component artifact against the versioned WIT runtime.
pub fn verify_component(component: &Path) -> anyhow::Result<Value> {
    soma_provider_adapters::wasm::verify_component_artifact(component)
        .map_err(anyhow::Error::msg)?;
    Ok(json!({"ok": true, "component": component, "wit": "soma:provider@1.0.0"}))
}

/// Replay recorded Python results against a component artifact.
pub fn compare(component: &Path, fixtures: &Path) -> anyhow::Result<Value> {
    let fixtures: Vec<GraduationFixture> = serde_json::from_slice(&fs::read(fixtures)?)?;
    if fixtures.is_empty() {
        anyhow::bail!("graduation comparison requires at least one fixture");
    }
    let capabilities = soma_provider_core::HostCapabilities::default();
    let results = fixtures
        .iter()
        .map(|fixture| {
            let actual = soma_provider_adapters::wasm::invoke_component_artifact(
                component,
                &fixture.input,
                &capabilities,
            );
            json!({
                "name": fixture.name,
                "matches": actual.as_ref().is_ok_and(|actual| actual == &fixture.expected),
                "expected": fixture.expected,
                "actual": actual.as_ref().ok(),
                "error": actual.as_ref().err(),
            })
        })
        .collect::<Vec<_>>();
    let matches = results.iter().all(|result| result["matches"] == true);
    Ok(json!({"ok": matches, "fixtures": results}))
}

/// Atomically activate the verified candidate while retaining one rollback.
pub fn activate(workspace: &Path) -> anyhow::Result<Value> {
    let mut state = read_state(workspace)?;
    let candidate = state
        .candidate
        .take()
        .ok_or_else(|| anyhow::anyhow!("no verified component candidate exists"))?;
    soma_provider_adapters::wasm::verify_component_artifact(&candidate)
        .map_err(anyhow::Error::msg)?;
    state.previous = state.active.replace(candidate.clone());
    write_state(workspace, &state)?;
    Ok(json!({"ok": true, "active": candidate, "previous": state.previous}))
}

/// Atomically reactivate the retained previous component.
pub fn rollback(workspace: &Path) -> anyhow::Result<Value> {
    let mut state = read_state(workspace)?;
    let previous = state
        .previous
        .take()
        .ok_or_else(|| anyhow::anyhow!("no retained component exists for rollback"))?;
    soma_provider_adapters::wasm::verify_component_artifact(&previous)
        .map_err(anyhow::Error::msg)?;
    state.previous = state.active.replace(previous.clone());
    write_state(workspace, &state)?;
    Ok(json!({"ok": true, "active": previous, "previous": state.previous}))
}

fn state_path(workspace: &Path) -> PathBuf {
    workspace.join("graduation.json")
}

fn read_state(workspace: &Path) -> anyhow::Result<GraduationState> {
    Ok(serde_json::from_slice(&fs::read(state_path(workspace))?)?)
}

fn write_state(workspace: &Path, state: &GraduationState) -> anyhow::Result<()> {
    let destination = state_path(workspace);
    let staging = destination.with_extension(format!("staging-{}", unique_suffix()));
    fs::write(&staging, serde_json::to_vec_pretty(state)?)?;
    fs::rename(staging, destination)?;
    Ok(())
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graduate_is_atomic_and_never_claims_translation() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let source = temp.path().join("provider.py");
        fs::write(&source, "def business_logic(): return 42\n").expect("source");
        let workspace = temp.path().join("graduated");
        let report = graduate(&source, &workspace, None).expect("graduation scaffold");
        assert_eq!(report["translated_business_logic"], false);
        assert_eq!(report["manual_rewrite_required"], true);
        assert!(workspace.join("source.py").is_file());
        assert!(
            fs::read_to_string(workspace.join("src/core.rs"))
                .unwrap()
                .contains("manual rewrite required")
        );
        assert!(workspace.join("src/component.rs").is_file());
        assert!(workspace.join("src/python.rs").is_file());
        assert!(workspace.join("wit/world.wit").is_file());
        assert!(graduate(&source, &workspace, None).is_err());
    }

    #[test]
    fn compare_requires_a_nonempty_recorded_fixture_set() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let fixtures = temp.path().join("fixtures.json");
        fs::write(&fixtures, "[]").expect("fixtures");
        let error = compare(Path::new("missing.wasm"), &fixtures).expect_err("empty corpus");
        assert!(error.to_string().contains("at least one fixture"));
    }
}
