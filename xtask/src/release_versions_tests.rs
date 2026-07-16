use super::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn manifest_models_single_soma_component() {
    let manifest = load_manifest(&repo_root()).expect("manifest");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.components.len(), 1);
    let component = &manifest.components[0];
    assert_eq!(component.id, "soma");
    assert_eq!(component.tag_prefix, "v");
    assert_eq!(component.release_workflow, "release.yml");
    assert!(component.shipping_paths.contains(&"crates".to_owned()));
    assert!(component
        .shipping_paths
        .contains(&"plugins/soma".to_owned()));
    assert!(component
        .version_files
        .iter()
        .any(|file| file.kind == VersionKind::JsonNoVersion));
    assert!(component.version_files.iter().any(|file| {
        file.kind == VersionKind::JsonVersion
            && file.path == "server.json"
            && file.json_pointer.as_deref() == Some("/packages/0/version")
    }));
    assert!(component.version_files.iter().any(|file| {
        file.kind == VersionKind::NpmIdentifierVersion
            && file.path == "server.json"
            && file.json_pointer.as_deref()
                == Some(
                    "/_meta/io.modelcontextprotocol.registry~1publisher-provided/distribution/npm",
                )
    }));
}

#[test]
fn exact_json_pointer_reads_nested_openapi_version() {
    let content = r#"{"version":"9.9.9","info":{"version":"1.2.3"}}"#;
    assert_eq!(
        read_json_version(content, Some("/info/version")).expect("version"),
        "1.2.3"
    );
}

#[test]
fn json_reader_allows_custom_comment_prefix() {
    let content = "<!-- CUSTOMIZE -->\n{\"version\":\"0.4.1\"}\n";
    assert_eq!(
        read_json_version(content, Some("/version")).expect("version"),
        "0.4.1"
    );
    let updated = replace_json_version(content, Some("/version"), "0.4.2").unwrap();
    assert!(updated.starts_with("<!-- CUSTOMIZE -->"));
    assert!(updated.contains("\"version\": \"0.4.2\""));
}

#[test]
fn plugin_manifest_version_key_is_rejected_recursively() {
    let errors = check_json_no_version(r#"{"name":"x","nested":{"version":"1.0.0"}}"#)
        .expect_err("version key rejected");
    assert!(errors.to_string().contains("must not contain"));
}

#[test]
fn cargo_lock_package_version_round_trips() {
    let content = r#"# generated
[[package]]
name = "soma"
version = "0.4.1"

[[package]]
name = "xtask"
version = "0.1.0"
"#;
    assert_eq!(
        read_cargo_lock_package_version(content, Some("soma")).unwrap(),
        "0.4.1"
    );
    let updated = replace_cargo_lock_package_version(content, Some("soma"), "0.4.2").unwrap();
    assert!(updated.contains("version = \"0.4.2\""));
    assert!(updated.contains("name = \"xtask\"\nversion = \"0.1.0\""));
}

#[test]
fn oci_identifier_version_uses_tag_suffix() {
    let content = r#"{"packages":[{"identifier":"ghcr.io/jmagar/soma:0.4.1"}]}"#;
    assert_eq!(
        read_oci_identifier_version(content, Some("/packages/0/identifier")).unwrap(),
        "0.4.1"
    );
    let updated =
        replace_oci_identifier_version(content, Some("/packages/0/identifier"), "0.4.2").unwrap();
    assert!(updated.contains("ghcr.io/jmagar/soma:0.4.2"));
}

#[test]
fn npm_identifier_version_uses_package_specifier_suffix() {
    let content = r#"{"_meta":{"io.modelcontextprotocol.registry/publisher-provided":{"distribution":{"npm":"@scope/soma-rmcp@0.4.1"}}}}"#;
    let pointer =
        Some("/_meta/io.modelcontextprotocol.registry~1publisher-provided/distribution/npm");
    assert_eq!(
        read_npm_identifier_version(content, pointer).unwrap(),
        "0.4.1"
    );
    let updated = replace_npm_identifier_version(content, pointer, "0.4.2").unwrap();
    assert!(updated.contains("@scope/soma-rmcp@0.4.2"));
}

#[test]
fn release_please_manifest_sync_updates_all_version_files() {
    let fixture = Fixture::new();
    fs::write(
        fixture.path(".release-please-manifest.json"),
        r#"{".":"0.4.2"}"#,
    )
    .unwrap();

    sync_release_please_version(fixture.root(), "soma").unwrap();
    check_version_sync(fixture.root()).unwrap();

    let server = fs::read_to_string(fixture.path("server.json")).unwrap();
    assert!(server.contains(r#""version": "0.4.2""#));
    assert!(server.contains("soma-rmcp@0.4.2"));
    assert!(fs::read_to_string(fixture.path("apps/soma/Cargo.toml"))
        .unwrap()
        .contains(r#"version = "0.4.2""#));
    assert!(fs::read_to_string(fixture.path("Cargo.lock"))
        .unwrap()
        .contains(r#"version = "0.4.2""#));
}

#[test]
fn parity_checks_registry_openapi_and_plugin_no_version() {
    let fixture = Fixture::new();
    fs::write(
        fixture.path("server.json"),
        r#"{"version":"0.4.0","_meta":{"io.modelcontextprotocol.registry/publisher-provided":{"distribution":{"npm":"soma-rmcp@0.4.1","nodePackage":"soma-rmcp"}}},"packages":[{"identifier":"soma-rmcp","version":"0.4.1"}]}"#,
    )
    .unwrap();
    fs::write(
        fixture.path("docs/generated/openapi.json"),
        r#"{"info":{"version":"0.4.1"}}"#,
    )
    .unwrap();
    fs::write(
        fixture.path("plugins/soma/.claude-plugin/plugin.json"),
        r#"{"name":"soma","version":"0.4.1"}"#,
    )
    .unwrap();
    let manifest = load_manifest(fixture.root()).unwrap();
    let errors = check_component_parity(fixture.root(), &manifest.components[0], "0.4.1").unwrap();
    assert!(errors.iter().any(|error| error.contains("server.json")));
    assert!(errors
        .iter()
        .any(|error| error.contains("must not contain")));
}

#[test]
fn shipping_change_requires_version_greater_than_latest_tag() {
    let fixture = Fixture::new();
    fixture.init_repo();
    fixture.git(&["tag", "v0.4.1"]);
    fs::write(
        fixture.path("apps/soma/src/lib.rs"),
        "pub fn changed() {}\n",
    )
    .unwrap();
    fixture.git(&["add", "apps/soma/src/lib.rs"]);
    fixture.git(&["commit", "-m", "change source"]);

    let error = check(fixture.root(), Some("v0.4.1"), "HEAD", GateMode::Pr, false)
        .expect_err("unchanged version should fail");
    assert!(error.to_string().contains("release version check failed"));
}

#[test]
fn docs_only_change_does_not_require_bump() {
    let fixture = Fixture::new();
    fixture.init_repo();
    fixture.git(&["tag", "v0.4.1"]);
    fs::create_dir_all(fixture.path("docs")).unwrap();
    fs::write(fixture.path("docs/note.md"), "docs only\n").unwrap();
    fixture.git(&["add", "docs/note.md"]);
    fixture.git(&["commit", "-m", "docs"]);

    check(fixture.root(), Some("v0.4.1"), "HEAD", GateMode::Pr, false)
        .expect("docs-only change is allowed");
}

#[test]
fn pr_mode_uses_merge_base_not_direct_base_diff() {
    let fixture = Fixture::new();
    fixture.init_repo();
    fixture.git(&["checkout", "-b", "feature"]);
    fs::create_dir_all(fixture.path("docs")).unwrap();
    fs::write(fixture.path("docs/note.md"), "feature docs\n").unwrap();
    fixture.git(&["add", "docs/note.md"]);
    fixture.git(&["commit", "-m", "docs"]);
    fixture.git(&["checkout", "main"]);
    fs::write(
        fixture.path("apps/soma/src/lib.rs"),
        "pub fn main_changed() {}\n",
    )
    .unwrap();
    fixture.git(&["add", "apps/soma/src/lib.rs"]);
    fixture.git(&["commit", "-m", "main source change"]);
    fixture.git(&["checkout", "feature"]);

    check(fixture.root(), Some("main"), "HEAD", GateMode::Pr, false)
        .expect("base-only shipping changes do not force feature bump");
}

#[test]
fn main_mode_uses_latest_semver_tag() {
    let fixture = Fixture::new();
    fixture.init_repo();
    fixture.git(&["tag", "v0.4.0"]);
    fixture.git(&["tag", "v0.4.1"]);
    fs::write(
        fixture.path("apps/soma/src/lib.rs"),
        "pub fn changed() {}\n",
    )
    .unwrap();
    fixture.git(&["add", "apps/soma/src/lib.rs"]);
    fixture.git(&["commit", "-m", "change source"]);

    let plans = plan(fixture.root(), None, "HEAD", GateMode::Main).unwrap();
    assert_eq!(plans[0].last_tag.as_deref(), Some("v0.4.1"));
    assert!(plans[0].changed);
}

struct Fixture {
    temp: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("tempdir");
        let fixture = Self { temp };
        fixture.write_minimal_tree();
        fixture
    }

    fn root(&self) -> &Path {
        self.temp.path()
    }

    fn path(&self, path: &str) -> PathBuf {
        self.root().join(path)
    }

    fn init_repo(&self) {
        self.git(&["init", "-b", "main"]);
        self.git(&["config", "user.email", "test@example.com"]);
        self.git(&["config", "user.name", "Test User"]);
        self.git(&["add", "."]);
        self.git(&["commit", "-m", "initial"]);
    }

    fn git(&self, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(self.root())
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {:?} failed", args);
    }

    fn write_minimal_tree(&self) {
        write(
            &self.path("release/components.toml"),
            include_str!("../../release/components.toml"),
        );
        write(
            &self.path("Cargo.toml"),
            r#"[workspace]
members = ["crates/soma"]
"#,
        );
        write(
            &self.path("apps/soma/Cargo.toml"),
            r#"[package]
name = "soma"
version = "0.4.1"
"#,
        );
        write(
            &self.path("Cargo.lock"),
            r#"[[package]]
name = "soma"
version = "0.4.1"
"#,
        );
        write(&self.path("CHANGELOG.md"), "# Changelog\n\n## [0.4.1]\n");
        write(
            &self.path(".release-please-manifest.json"),
            r#"{".":"0.4.1"}"#,
        );
        write(
            &self.path("server.json"),
            r#"{"version":"0.4.1","_meta":{"io.modelcontextprotocol.registry/publisher-provided":{"distribution":{"npm":"soma-rmcp@0.4.1","nodePackage":"soma-rmcp"}}},"packages":[{"identifier":"soma-rmcp","version":"0.4.1"}]}"#,
        );
        write(
            &self.path("docs/generated/openapi.json"),
            r#"{"info":{"version":"0.4.1"}}"#,
        );
        write(
            &self.path("packages/soma-rmcp/package.json"),
            r#"{"name":"soma-rmcp","version":"0.4.1"}"#,
        );
        write(
            &self.path("plugins/soma/.claude-plugin/plugin.json"),
            r#"{"name":"soma"}"#,
        );
        write(
            &self.path("plugins/soma/.codex-plugin/plugin.json"),
            r#"{"name":"soma"}"#,
        );
        write(
            &self.path("plugins/soma/gemini-extension.json"),
            r#"{"name":"soma"}"#,
        );
        write(&self.path("apps/soma/src/lib.rs"), "pub fn original() {}\n");
        write(&self.path("apps/web/.keep"), "");
        write(&self.path("config/Dockerfile"), "");
        write(&self.path("entrypoint.sh"), "");
        write(&self.path("install.sh"), "");
        write(&self.path("scripts/repair.sh"), "");
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask parent")
        .to_path_buf()
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}
