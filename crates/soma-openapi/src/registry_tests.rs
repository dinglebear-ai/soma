use std::time::Duration;

use crate::config::{OpenApiConfig, OpenApiSpecConfig, SpecSource};
use crate::registry::{OpenApiRegistry, MAX_SPEC_BYTES};

const FIXTURE_SPEC: &str = r#"{
    "openapi": "3.0.0",
    "info": { "title": "Fixture", "version": "1.0.0" },
    "paths": {
        "/users/{id}": {
            "get": {
                "operationId": "getUser",
                "responses": { "200": { "description": "ok" } }
            }
        }
    }
}"#;

fn fixture_path(label: &str, body: &str) -> std::path::PathBuf {
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join(format!("{label}.json"));
    std::fs::write(&path, body).unwrap();
    path
}

fn good_fixture_spec(label: &str) -> OpenApiSpecConfig {
    OpenApiSpecConfig {
        label: label.into(),
        spec_source: SpecSource::Path(fixture_path(label, FIXTURE_SPEC)),
        base_url: "https://api.example.com".parse().unwrap(),
        allowed_operations: vec!["getUser".into()],
        credential: None,
    }
}

fn bad_spec(label: &str, base: &str) -> OpenApiSpecConfig {
    OpenApiSpecConfig {
        label: label.into(),
        spec_source: SpecSource::Path(std::env::temp_dir().join("missing-openapi.json")),
        base_url: base.parse().unwrap(),
        allowed_operations: vec![],
        credential: None,
    }
}

#[tokio::test]
async fn registry_omits_bad_spec_without_blocking_good_spec() {
    let cfg = OpenApiConfig {
        specs: vec![
            good_fixture_spec("goodlabel"),
            bad_spec("badlabel", "https://10.255.255.1"),
        ],
    };
    let started = std::time::Instant::now();
    let reg = OpenApiRegistry::from_config(cfg, Duration::from_secs(2)).await;
    assert!(reg.labels().contains(&"goodlabel".to_string()));
    assert!(!reg.labels().contains(&"badlabel".to_string()));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn good_spec_exposes_allowed_operation() {
    let cfg = OpenApiConfig {
        specs: vec![good_fixture_spec("vendor")],
    };
    let reg = OpenApiRegistry::from_config(cfg, Duration::from_secs(2)).await;
    let op = reg.operation("vendor", "getUser").expect("op present");
    assert_eq!(op.method, reqwest::Method::GET);
    assert_eq!(op.path_template, "/users/{id}");
    assert_eq!(
        reg.operation("vendor", "nope").unwrap_err().kind(),
        "unknown_action"
    );
    assert_eq!(
        reg.operation("nope", "getUser").unwrap_err().kind(),
        "unknown_instance"
    );
}

#[tokio::test]
async fn path_spec_too_large_is_rejected_before_full_read() {
    let path = fixture_path("too-large", &"x".repeat(MAX_SPEC_BYTES + 1));
    let cfg = OpenApiConfig {
        specs: vec![OpenApiSpecConfig {
            label: "huge".into(),
            spec_source: SpecSource::Path(path),
            base_url: "https://api.example.com".parse().unwrap(),
            allowed_operations: vec!["getUser".into()],
            credential: None,
        }],
    };
    let reg = OpenApiRegistry::from_config(cfg, Duration::from_secs(2)).await;
    assert!(reg.is_empty());
}

#[tokio::test]
async fn reserved_labels_are_omitted() {
    let mut spec = good_fixture_spec("state");
    spec.label = "state".into();
    let reg =
        OpenApiRegistry::from_config(OpenApiConfig { specs: vec![spec] }, Duration::from_secs(2))
            .await;
    assert!(reg.is_empty());
}

#[test]
fn openapi_source_does_not_reference_codemode_tool_error() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root];
    let forbidden_error = concat!("Tool", "Error");
    let forbidden_crate = concat!("soma", "_codemode");
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(!text.contains(forbidden_error), "{}", path.display());
                assert!(!text.contains(forbidden_crate), "{}", path.display());
            }
        }
    }
}
