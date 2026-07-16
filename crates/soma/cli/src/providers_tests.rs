use std::fs;

use tempfile::tempdir;

use super::{build_provider_report_json, build_provider_report_text};
use crate::ProviderCommand;

#[test]
fn providers_list_text_includes_loaded_provider_actions() {
    let temp = tempdir().expect("tempdir");
    fs::write(
        temp.path().join("hello.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "hello", "kind": "static-rust", "version": "0.1.0" },
          "tools": [
            {
              "name": "hello",
              "description": "Hello probe",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
              "output_schema": { "type": "object", "properties": {}, "additionalProperties": true }
            }
          ]
        }"#,
    )
    .expect("write provider");

    let output = build_provider_report_text(&ProviderCommand::List {
        dir: Some(temp.path().to_path_buf()),
        json: false,
    })
    .expect("build report");

    assert!(output.contains("Provider directory:"));
    assert!(output.contains("hello.json"));
    assert!(output.contains("hello"));
}

#[test]
fn providers_lint_json_marks_invalid_files_and_returns_valid_false() {
    let temp = tempdir().expect("tempdir");
    fs::write(temp.path().join("broken.json"), "{").expect("write invalid provider");

    let value = build_provider_report_json(&ProviderCommand::Lint {
        dir: Some(temp.path().to_path_buf()),
        json: true,
    })
    .expect("build report");

    assert_eq!(value["valid"], false);
    assert_eq!(value["summary"]["invalid"], 1);
    assert_eq!(value["files"][0]["status"], "invalid");
}
