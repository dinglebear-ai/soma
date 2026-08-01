use std::{fs, path::Path};

use serde_json::json;

use super::*;

fn graduation_fixture(root: &Path) -> (std::path::PathBuf, GraduationState) {
    let providers = root.join("providers");
    let workspace = root.join("graduation");
    fs::create_dir_all(&providers).expect("provider root");
    fs::create_dir_all(&workspace).expect("workspace");
    let source = providers.join("example.py");
    fs::write(
        &source,
        "def echo(value):
    return value
",
    )
    .expect("source");
    let catalog: soma_provider_core::ProviderCatalog = serde_json::from_value(json!({
        "schema_version": 1,
        "provider": {"name": "example", "kind": "python", "source": source},
        "tools": [{
            "name": "echo",
            "description": "Echo a value.",
            "input_schema": {"type": "object"},
            "cli": {"enabled": true, "command": "echo"},
            "meta": {"python": {"adapter": "python"}}
        }]
    }))
    .expect("catalog");
    let state = GraduationState {
        schema_version: 3,
        source: source.clone(),
        source_sha256: digest_file(&source, MAX_SOURCE_BYTES).expect("source digest"),
        catalog_sha256: crate::graduation::catalog_contract_digest(&catalog)
            .expect("catalog digest"),
        catalog,
        candidate: None,
        active: None,
        previous: None,
        python_backup: None,
        attestation: None,
    };
    fs::write(
        workspace.join("graduation.json"),
        serde_json::to_vec_pretty(&state).expect("state JSON"),
    )
    .expect("state");
    (workspace, state)
}

fn componentize_state(workspace: &Path, graduation: &GraduationState) -> ComponentizeState {
    let report = json!({
        "schema_version": 2,
        "policy_version": POLICY_VERSION,
        "componentize_py_version": COMPONENTIZE_PY_VERSION,
        "experimental": true,
        "compatible": true,
        "requires_build_validation": true,
        "filename": graduation.source,
        "source_sha256": graduation.source_sha256,
        "imports": [],
        "external_imports": [],
        "import_distributions": {},
        "wheel_files": [],
        "wheel_evidence": [],
        "findings": []
    });
    let bytes = serde_json::to_vec_pretty(&report).expect("report");
    fs::write(workspace.join(REPORT_FILE), &bytes).expect("report file");
    ComponentizeState {
        schema_version: STATE_SCHEMA_VERSION,
        policy_version: POLICY_VERSION.to_owned(),
        componentize_py_version: COMPONENTIZE_PY_VERSION.to_owned(),
        source: graduation.source.clone(),
        source_sha256: graduation.source_sha256.clone(),
        wheelhouse: None,
        wheels: Vec::new(),
        report_sha256: digest(&bytes),
        compatible: true,
        bindings: None,
        component: None,
        graduation_candidate: None,
        verified: false,
        verified_unix_ms: None,
    }
}

#[test]
fn status_is_unconfigured_until_a_scan_is_persisted() {
    let root = tempfile::tempdir().expect("root");
    let (workspace, _) = graduation_fixture(root.path());
    let report = status(&workspace, &root.path().join("providers")).expect("status");
    assert_eq!(report["configured"], false);
    assert_eq!(report["policy_version"], POLICY_VERSION);
}

#[test]
fn state_is_source_and_report_digest_bound() {
    let root = tempfile::tempdir().expect("root");
    let (workspace, graduation) = graduation_fixture(root.path());
    let state = componentize_state(&workspace, &graduation);
    write_state(&workspace, &state).expect("componentize state");
    validate_state(&workspace, &graduation, &state, true).expect("valid state");

    fs::write(
        &graduation.source,
        "def changed():
    return 2
",
    )
    .expect("changed source");
    assert!(
        validate_state(&workspace, &graduation, &state, true)
            .expect_err("source drift")
            .to_string()
            .contains("source changed")
    );
}

#[test]
fn report_tampering_is_rejected() {
    let root = tempfile::tempdir().expect("root");
    let (workspace, graduation) = graduation_fixture(root.path());
    let state = componentize_state(&workspace, &graduation);
    write_state(&workspace, &state).expect("componentize state");
    fs::write(workspace.join(REPORT_FILE), b"{}").expect("tampered report");

    assert!(
        validate_state(&workspace, &graduation, &state, true)
            .expect_err("report drift")
            .to_string()
            .contains("report digest mismatch")
    );
}

#[test]
fn directory_digest_is_content_and_path_sensitive() {
    let root = tempfile::tempdir().expect("root");
    let first = root.path().join("first");
    fs::create_dir(&first).expect("first");
    fs::write(first.join("a.py"), b"one").expect("a");
    let digest_one = directory_digest(&first).expect("digest one");
    fs::write(first.join("a.py"), b"two").expect("a changed");
    let digest_two = directory_digest(&first).expect("digest two");
    fs::rename(first.join("a.py"), first.join("b.py")).expect("rename");
    let digest_three = directory_digest(&first).expect("digest three");

    assert_ne!(digest_one, digest_two);
    assert_ne!(digest_two, digest_three);
}

#[test]
fn compatible_scanner_evidence_must_match_the_scanned_wheel_set() {
    let root = tempfile::tempdir().expect("root");
    let (_, graduation) = graduation_fixture(root.path());
    let wheel = root.path().join("example-1.0.0-py3-none-any.whl");
    fs::write(&wheel, b"wheel").expect("wheel");
    let wheels = vec![wheel.canonicalize().expect("canonical wheel")];
    let report = ScannerReport {
        schema_version: 2,
        policy_version: POLICY_VERSION.to_owned(),
        componentize_py_version: COMPONENTIZE_PY_VERSION.to_owned(),
        experimental: true,
        compatible: true,
        requires_build_validation: true,
        filename: graduation.source.display().to_string(),
        source_sha256: graduation.source_sha256.clone(),
        imports: Vec::new(),
        external_imports: Vec::new(),
        import_distributions: BTreeMap::new(),
        wheel_files: wheels.clone(),
        wheel_evidence: Vec::new(),
        findings: Vec::new(),
    };

    assert!(
        validate_report(&report, &graduation, &wheels)
            .expect_err("missing wheel evidence")
            .to_string()
            .contains("not bound")
    );
}

#[test]
fn incompatible_scans_preserve_partial_evidence_and_findings() {
    let root = tempfile::tempdir().expect("root");
    let (_, graduation) = graduation_fixture(root.path());
    let wheel = root.path().join("broken-1.0.0-py3-none-any.whl");
    fs::write(&wheel, b"not a zip archive").expect("wheel");
    let wheels = vec![wheel.canonicalize().expect("canonical wheel")];
    let report = ScannerReport {
        schema_version: 2,
        policy_version: POLICY_VERSION.to_owned(),
        componentize_py_version: COMPONENTIZE_PY_VERSION.to_owned(),
        experimental: true,
        compatible: false,
        requires_build_validation: false,
        filename: graduation.source.display().to_string(),
        source_sha256: graduation.source_sha256.clone(),
        imports: vec!["broken".to_owned()],
        external_imports: vec!["broken".to_owned()],
        import_distributions: BTreeMap::new(),
        wheel_files: wheels.clone(),
        wheel_evidence: Vec::new(),
        findings: vec![json!({
            "code": "dependency_wheel_invalid",
            "severity": "error",
            "message": "invalid wheel",
            "line": null,
            "subject": wheel.display().to_string(),
        })],
    };

    validate_report(&report, &graduation, &wheels)
        .expect("incompatible findings must remain persistable");
}

#[test]
fn invalid_wheel_scan_persists_findings_and_blocks_build_progression() {
    let root = tempfile::tempdir().expect("root");
    let (workspace, graduation) = graduation_fixture(root.path());
    let wheelhouse = root.path().join("wheelhouse");
    fs::create_dir(&wheelhouse).expect("wheelhouse");
    let wheel = wheelhouse.join("broken-1.0.0-py3-none-any.whl");
    fs::write(&wheel, b"not a zip archive").expect("wheel");

    let report = scan(
        &workspace,
        Some(&wheelhouse),
        &root.path().join("providers"),
    )
    .expect("incompatible scan must remain reportable");
    assert_eq!(report["compatible"], false);
    assert!(
        report["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "dependency_wheel_invalid")
    );
    assert!(workspace.join(REPORT_FILE).is_file());

    let state = read_state_file(&workspace).expect("persisted componentize state");
    assert!(!state.compatible);
    let status = status(&workspace, &root.path().join("providers")).expect("scan status");
    assert_eq!(status["configured"], true);
    assert_eq!(status["valid"], true);
    assert_eq!(status["state"]["compatible"], false);
    assert!(
        load_valid_state(&workspace, &graduation, true)
            .expect_err("incompatible scan cannot build")
            .to_string()
            .contains("blocking findings")
    );
}

#[test]
fn incompatible_reports_require_an_error_finding() {
    let root = tempfile::tempdir().expect("root");
    let (_, graduation) = graduation_fixture(root.path());
    let report = ScannerReport {
        schema_version: 2,
        policy_version: POLICY_VERSION.to_owned(),
        componentize_py_version: COMPONENTIZE_PY_VERSION.to_owned(),
        experimental: true,
        compatible: false,
        requires_build_validation: false,
        filename: graduation.source.display().to_string(),
        source_sha256: graduation.source_sha256.clone(),
        imports: Vec::new(),
        external_imports: Vec::new(),
        import_distributions: BTreeMap::new(),
        wheel_files: Vec::new(),
        wheel_evidence: Vec::new(),
        findings: Vec::new(),
    };

    assert!(
        validate_report(&report, &graduation, &[])
            .expect_err("incompatible report without an error")
            .to_string()
            .contains("does not match")
    );
}

#[test]
fn incompatible_scans_cannot_advance_to_build_inputs() {
    let root = tempfile::tempdir().expect("root");
    let (workspace, graduation) = graduation_fixture(root.path());
    let mut state = componentize_state(&workspace, &graduation);
    state.compatible = false;
    write_state(&workspace, &state).expect("componentize state");

    assert!(
        load_valid_state(&workspace, &graduation, true)
            .expect_err("incompatible state")
            .to_string()
            .contains("blocking findings")
    );
}
