use std::fs;

use tempfile::tempdir;

use super::*;
use crate::provider_registry::ResourceReadOutput;

#[test]
fn static_markdown_file_derives_uri_name_and_heading_description() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let path = temp.path().join("runbook.md");
    fs::write(&path, "# On-Call Runbook\n\nDo the thing.\n").expect("write file");

    let provider = ResourceFileProvider::from_file(path.clone(), Path::new("runbook.md"), &root)
        .expect("build");
    let catalog = provider.catalog();
    assert_eq!(catalog.resources.len(), 1);
    let resource = &catalog.resources[0];
    assert_eq!(resource.uri_template, "soma://resources/runbook");
    assert_eq!(resource.name, "runbook");
    assert_eq!(resource.description, "On-Call Runbook");
    assert_eq!(resource.mime_type.as_deref(), Some("text/markdown"));
}

#[test]
fn nested_static_resource_maps_path_to_uri() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let dir = temp.path().join("api");
    fs::create_dir(&dir).expect("mkdir");
    let path = dir.join("schema.json");
    fs::write(&path, r#"{"ok":true}"#).expect("write file");

    let provider =
        ResourceFileProvider::from_file(path, Path::new("api/schema.json"), &root).expect("build");
    let resource = &provider.catalog().resources[0];
    assert_eq!(resource.uri_template, "soma://resources/api/schema");
    assert_eq!(resource.mime_type.as_deref(), Some("application/json"));
}

#[test]
fn nested_static_resources_sharing_a_leaf_name_get_distinct_names() {
    // Regression: naming a static resource from just the leaf stem made
    // resources/api/runbook.md and resources/ops/runbook.md both derive
    // name == "runbook" despite having different, non-colliding URIs --
    // the global resource-name uniqueness check in build_snapshot() would
    // then spuriously reject the second one and fail the whole refresh.
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let api_dir = temp.path().join("api");
    let ops_dir = temp.path().join("ops");
    fs::create_dir(&api_dir).expect("mkdir api");
    fs::create_dir(&ops_dir).expect("mkdir ops");
    fs::write(api_dir.join("runbook.md"), "# API Runbook\n").expect("write api runbook");
    fs::write(ops_dir.join("runbook.md"), "# Ops Runbook\n").expect("write ops runbook");

    let api_provider = ResourceFileProvider::from_file(
        api_dir.join("runbook.md"),
        Path::new("api/runbook.md"),
        &root,
    )
    .expect("build api provider");
    let ops_provider = ResourceFileProvider::from_file(
        ops_dir.join("runbook.md"),
        Path::new("ops/runbook.md"),
        &root,
    )
    .expect("build ops provider");

    let api_resource = &api_provider.catalog().resources[0];
    let ops_resource = &ops_provider.catalog().resources[0];
    assert_ne!(
        api_resource.name, ops_resource.name,
        "distinct URIs must not derive the same resource name"
    );
    assert_eq!(api_resource.uri_template, "soma://resources/api/runbook");
    assert_eq!(ops_resource.uri_template, "soma://resources/ops/runbook");
}

#[tokio::test]
async fn read_static_text_resource_returns_text_contents() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let path = temp.path().join("notes.txt");
    fs::write(&path, "hello world").expect("write file");

    let provider =
        ResourceFileProvider::from_file(path, Path::new("notes.txt"), &root).expect("build");
    let output = provider
        .read_resource("soma://resources/notes", &BTreeMap::new())
        .await
        .expect("read should succeed");
    match output {
        ResourceReadOutput::Text { text, mime_type } => {
            assert_eq!(text, "hello world");
            assert_eq!(mime_type.as_deref(), Some("text/plain"));
        }
        ResourceReadOutput::Blob { .. } => panic!("expected text output"),
    }
}

#[tokio::test]
async fn read_static_binary_resource_returns_base64_blob() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let path = temp.path().join("logo.png");
    fs::write(&path, [0x89, 0x50, 0x4e, 0x47]).expect("write file");

    let provider =
        ResourceFileProvider::from_file(path, Path::new("logo.png"), &root).expect("build");
    let output = provider
        .read_resource("soma://resources/logo", &BTreeMap::new())
        .await
        .expect("read should succeed");
    match output {
        ResourceReadOutput::Blob {
            blob_base64,
            mime_type,
        } => {
            assert_eq!(mime_type.as_deref(), Some("image/png"));
            assert_eq!(
                BASE64.decode(blob_base64).unwrap(),
                vec![0x89, 0x50, 0x4e, 0x47]
            );
        }
        ResourceReadOutput::Text { .. } => panic!("expected blob output"),
    }
}

#[test]
fn oversized_static_resource_is_rejected_at_discovery() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let path = temp.path().join("big.txt");
    // Sparse file: doesn't actually allocate MAX_STATIC_RESOURCE_BYTES+1 on
    // disk, but metadata().len() still reports the logical size, which is
    // all from_file()'s size check reads.
    let file = fs::File::create(&path).expect("create file");
    file.set_len(MAX_STATIC_RESOURCE_BYTES + 1)
        .expect("set sparse length");

    let error = ResourceFileProvider::from_file(path, Path::new("big.txt"), &root)
        .expect_err("oversized file must be rejected");
    assert!(error.0.contains("exceeds"));
}

#[test]
fn dynamic_ts_file_becomes_a_template_not_a_static_resource() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let dir = temp.path().join("service");
    fs::create_dir(&dir).expect("mkdir");
    let path = dir.join("[name].ts");
    fs::write(
        &path,
        "export async function read(input) { return { text: input.params.name }; }",
    )
    .expect("write file");

    let provider = ResourceFileProvider::from_file(path, Path::new("service/[name].ts"), &root)
        .expect("build");
    let catalog = provider.catalog();
    assert!(
        catalog.resources.is_empty(),
        "dynamic readers are not static resources"
    );
    let templates = provider.dynamic_resource_templates();
    assert_eq!(templates.len(), 1);
    assert_eq!(
        templates[0].path.uri_string(),
        "soma://resources/service/{name}"
    );
    assert_eq!(
        templates[0].scope.as_deref(),
        Some("soma:write"),
        "dynamic readers execute code and must default to the stricter scope"
    );
}

#[test]
fn static_resource_file_rejects_bracket_path_segments() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let dir = temp.path().join("service");
    fs::create_dir(&dir).expect("mkdir");
    let path = dir.join("[name].json");
    fs::write(&path, "{}").expect("write file");

    let error = ResourceFileProvider::from_file(path, Path::new("service/[name].json"), &root)
        .expect_err("only .ts files may use bracket segments");
    assert!(error.0.contains("bracketed"));
}

#[test]
fn mime_inference_covers_the_contract_examples() {
    assert_eq!(
        mime_type_for_extension(Path::new("runbook.md")),
        "text/markdown"
    );
    assert_eq!(
        mime_type_for_extension(Path::new("notes.txt")),
        "text/plain"
    );
    assert_eq!(
        mime_type_for_extension(Path::new("api/schema.json")),
        "application/json"
    );
    assert_eq!(
        mime_type_for_extension(Path::new("images/logo.png")),
        "image/png"
    );
    assert_eq!(
        mime_type_for_extension(Path::new("unknown.bin")),
        "application/octet-stream"
    );
}

#[test]
fn parse_reader_output_accepts_all_three_shapes() {
    let text = parse_reader_output(
        "p",
        "u",
        &serde_json::json!({"text": "hi", "mimeType": "text/plain"}),
    )
    .expect("text shape");
    assert!(matches!(text, ResourceReadOutput::Text { text, .. } if text == "hi"));

    let json = parse_reader_output("p", "u", &serde_json::json!({"json": {"ok": true}}))
        .expect("json shape");
    assert!(
        matches!(json, ResourceReadOutput::Text { mime_type, .. } if mime_type.as_deref() == Some("application/json"))
    );

    let blob = parse_reader_output(
        "p",
        "u",
        &serde_json::json!({"blob": "AAA=", "mimeType": "image/png"}),
    )
    .expect("blob shape");
    assert!(
        matches!(blob, ResourceReadOutput::Blob { mime_type, .. } if mime_type.as_deref() == Some("image/png"))
    );
}

#[test]
fn parse_reader_output_rejects_blob_without_mime_type() {
    let error = parse_reader_output("p", "u", &serde_json::json!({"blob": "AAA="}))
        .expect_err("blob without mimeType must be rejected");
    assert!(error.message.contains("mimeType"));
}

#[test]
fn parse_reader_output_rejects_shapes_with_none_of_the_three_fields() {
    let error = parse_reader_output("p", "u", &serde_json::json!({"unexpected": true}))
        .expect_err("must reject unknown shape");
    assert!(error.message.contains("text"));
}

#[test]
fn parse_reader_output_rejects_non_object_top_level_values() {
    for value in [
        serde_json::json!(null),
        serde_json::json!([1, 2, 3]),
        serde_json::json!("oops"),
        serde_json::json!(true),
        serde_json::json!(42),
    ] {
        let error = parse_reader_output("p", "u", &value)
            .expect_err("non-object top-level value must be rejected");
        assert!(error.message.contains("object"), "{}", error.message);
    }
}

#[test]
fn parse_reader_output_rejects_non_string_text_field() {
    let error = parse_reader_output("p", "u", &serde_json::json!({"text": 42}))
        .expect_err("non-string text must be rejected");
    assert!(error.message.contains("`text`"));
}

#[tokio::test]
async fn read_static_resource_rejects_invalid_utf8_for_text_mime_types() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let path = temp.path().join("notes.txt");
    fs::write(&path, [0xff, 0xfe, 0x00, 0x01]).expect("write file");

    let provider =
        ResourceFileProvider::from_file(path, Path::new("notes.txt"), &root).expect("build");
    let error = provider
        .read_resource("soma://resources/notes", &BTreeMap::new())
        .await
        .expect_err("invalid UTF-8 must be rejected for a text MIME type");
    assert!(error.message.contains("UTF-8"));
}

#[test]
fn joined_segment_name_disambiguates_nesting_from_hyphenation() {
    // A hyphenated flat filename and a two-level nested directory both
    // produce the literal segment sequence that would collide if joined
    // with the same separator slugify() uses internally (`-`) — regression
    // for the bug where `resources/my-file.md` and `resources/my/file.md`
    // (different, non-colliding URIs) both flattened to the same
    // `provider_name`, spuriously tripping the directory-wide
    // duplicate-provider-name check.
    let flat = resource_uri::parse_resource_path(&["my-file"]).expect("parse flat");
    let nested = resource_uri::parse_resource_path(&["my", "file"]).expect("parse nested");
    assert_ne!(joined_segment_name(&flat), joined_segment_name(&nested));
    assert_ne!(flat.uri_string(), nested.uri_string());
}

#[tokio::test]
async fn read_resource_rejects_a_path_that_no_longer_resolves_within_the_root() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().canonicalize().expect("canonicalize temp root");
    let path = temp.path().join("notes.txt");
    fs::write(&path, "hello").expect("write file");
    let provider =
        ResourceFileProvider::from_file(path, Path::new("notes.txt"), &root).expect("build");

    // A root that doesn't contain the file at all simulates the file having
    // moved/been re-pointed outside the trust boundary between discovery
    // and read — verify_within_root must catch this at read time, not just
    // at discovery time.
    let outside_root = tempdir().expect("tempdir");
    let outside_root = outside_root
        .path()
        .canonicalize()
        .expect("canonicalize outside root");
    let error = read_static_resource(
        "p",
        "soma://resources/notes",
        provider.source_path(),
        &outside_root,
        "text/plain",
    )
    .expect_err("path outside the canonical root must be rejected");
    assert_eq!(error.code.as_ref(), "resource_escapes_root");
}
