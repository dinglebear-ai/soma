// Tests for the structured providers/{tools,prompts,resources}/ layout and
// the resources/ trust boundary, through the public FileProviderSource
// surface (this module's own resource_paths/walk_resources_dir have no
// public API of their own to unit-test directly).

use std::fs;

use tempfile::tempdir;

use super::super::{FileProviderSource, ProviderFileInspectionStatus};

#[test]
fn structured_tools_prompts_and_resources_directories_are_discovered() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    fs::create_dir(providers.join("tools")).expect("mkdir tools");
    fs::create_dir(providers.join("prompts")).expect("mkdir prompts");
    fs::create_dir(providers.join("resources")).expect("mkdir resources");

    fs::write(
        providers.join("tools").join("weather.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "weather", "kind": "static-rust" },
          "tools": [
            {
              "name": "weather_tool",
              "description": "probe",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false }
            }
          ]
        }"#,
    )
    .expect("write tools/weather.json");
    fs::write(
        providers.join("prompts").join("code-review.md"),
        "# Code Review\n\nReview it.\n",
    )
    .expect("write prompts/code-review.md");
    fs::write(
        providers.join("resources").join("runbook.md"),
        "# Runbook\n\nDo the thing.\n",
    )
    .expect("write resources/runbook.md");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 3, "errors: {:?}", report.files);
    assert_eq!(report.providers_invalid, 0);

    let weather = report
        .files
        .iter()
        .find(|file| file.provider_id.as_deref() == Some("weather"))
        .expect("weather tool discovered");
    assert_eq!(weather.actions, vec!["weather_tool"]);

    let prompt = report
        .files
        .iter()
        .find(|file| file.provider_id.as_deref() == Some("code-review"))
        .expect("code-review prompt discovered");
    assert_eq!(prompt.provider_kind.as_deref(), Some("static-rust"));

    let resource = report
        .files
        .iter()
        .find(|file| file.file_name == "runbook.md")
        .expect("runbook resource discovered");
    assert_eq!(resource.status, ProviderFileInspectionStatus::Loaded);
}

#[test]
fn root_level_files_still_load_alongside_structured_directories() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    fs::create_dir(providers.join("tools")).expect("mkdir tools");
    fs::write(
        providers.join("root-tool.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "root-tool", "kind": "static-rust" },
          "tools": [
            {
              "name": "root_action",
              "description": "probe",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false }
            }
          ]
        }"#,
    )
    .expect("write root-tool.json");
    fs::write(
        providers.join("tools").join("structured-tool.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "structured-tool", "kind": "static-rust" },
          "tools": [
            {
              "name": "structured_action",
              "description": "probe",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false }
            }
          ]
        }"#,
    )
    .expect("write tools/structured-tool.json");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 2, "errors: {:?}", report.files);
}

#[test]
fn tools_directory_ignores_markdown_files() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    fs::create_dir(providers.join("tools")).expect("mkdir tools");
    fs::write(providers.join("tools").join("notes.md"), "# Notes\n").expect("write notes.md");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert!(
        report.files.is_empty(),
        "providers/tools/ must not treat .md files as prompts: {:?}",
        report.files
    );
}

#[test]
fn nested_resource_files_map_to_nested_uris() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    let api_dir = providers.join("resources").join("api");
    fs::create_dir_all(&api_dir).expect("mkdir resources/api");
    fs::write(api_dir.join("schema.json"), r#"{"ok":true}"#).expect("write schema.json");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1, "errors: {:?}", report.files);
    let resource = &report.files[0];
    assert_eq!(resource.file_name, "api/schema.json");
    assert_eq!(resource.provider_kind.as_deref(), Some("static-rust"));
}

#[test]
fn dynamic_resource_reader_is_loaded_but_exposes_no_tool_actions() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    let service_dir = providers.join("resources").join("service");
    fs::create_dir_all(&service_dir).expect("mkdir resources/service");
    fs::write(
        service_dir.join("[name].ts"),
        "export async function read(input) { return { text: input.params.name }; }",
    )
    .expect("write [name].ts");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1, "errors: {:?}", report.files);
    assert!(report.files[0].actions.is_empty());
}

#[test]
fn lint_flags_colliding_dynamic_resource_readers() {
    // Regression: dynamic .ts resource templates never appear in
    // catalog().resources (they're derived from the filename, not declared
    // data), so the directory-wide uniqueness pass used to miss two
    // ambiguous readers entirely -- lint would report both as `Loaded`
    // even though the live ResourceIndex::register rejects the pair and
    // keeps the previous snapshot at real registry construction time.
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    let service_dir = providers.join("resources").join("service");
    fs::create_dir_all(&service_dir).expect("mkdir resources/service");
    fs::write(
        service_dir.join("[name].ts"),
        "export async function read(input) { return { text: input.params.name }; }",
    )
    .expect("write [name].ts");
    fs::write(
        service_dir.join("[id].ts"),
        "export async function read(input) { return { text: input.params.id }; }",
    )
    .expect("write [id].ts");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1, "errors: {:?}", report.files);
    assert_eq!(report.providers_invalid, 1, "errors: {:?}", report.files);
    let invalid = report
        .files
        .iter()
        .find(|file| file.status == ProviderFileInspectionStatus::Invalid)
        .expect("one reader should be flagged invalid");
    assert!(
        invalid
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("resource template"),
        "unexpected error: {:?}",
        invalid.error
    );
}

#[cfg(unix)]
#[test]
fn symlink_escaping_the_resources_root_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("tempdir");
    let outside = tempdir().expect("outside tempdir");
    fs::write(outside.path().join("secret.txt"), "leaked").expect("write outside file");

    let providers = temp.path();
    fs::create_dir(providers.join("resources")).expect("mkdir resources");
    symlink(
        outside.path().join("secret.txt"),
        providers.join("resources").join("escape.txt"),
    )
    .expect("create escaping symlink");

    let error = FileProviderSource::new(providers)
        .inspect()
        .expect_err("a resource symlink escaping the provider root must be rejected");
    assert!(
        error.message.contains("escapes the provider root"),
        "unexpected error: {}",
        error.message
    );
}

#[cfg(unix)]
#[test]
fn resources_directory_itself_being_a_symlink_escaping_the_root_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("tempdir");
    let outside = tempdir().expect("outside tempdir");
    fs::write(outside.path().join("secret.txt"), "leaked").expect("write outside file");

    let providers = temp.path();
    // Not a file/directory entry inside resources/ — the resources/
    // directory entry itself is a symlink pointing entirely outside the
    // provider root, which would defeat every subsequent
    // `starts_with(canonical_root)` check if canonical_root were derived
    // from the symlink's target instead of being checked against the
    // provider root first.
    symlink(outside.path(), providers.join("resources")).expect("create escaping symlink");

    let error = FileProviderSource::new(providers)
        .inspect()
        .expect_err("a resources/ directory symlink escaping the provider root must be rejected");
    assert!(
        error.message.contains("escapes the provider root"),
        "unexpected error: {}",
        error.message
    );
}

#[cfg(unix)]
#[test]
fn symlink_within_the_resources_root_is_accepted() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    fs::create_dir(providers.join("resources")).expect("mkdir resources");
    fs::write(providers.join("resources").join("real.txt"), "hello").expect("write real file");
    symlink(
        providers.join("resources").join("real.txt"),
        providers.join("resources").join("alias.txt"),
    )
    .expect("create internal symlink");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 2, "errors: {:?}", report.files);
}
