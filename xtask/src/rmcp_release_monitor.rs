//! `cargo xtask rmcp-release-monitor` - the scheduled drift monitor.
//!
//! Owns the orchestration only: read the CLI options, work out which rmcp
//! version this workspace pins, ask each watch whether its upstream moved, and
//! emit the GitHub Actions outputs. Everything each of those steps *does* lives
//! in a submodule.

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod conformance;
mod diagnostics;
mod impact;
mod issue_body;
mod options;
mod schema;

use conformance::{build_conformance_report, ConformanceMonitorInput};
use diagnostics::json_parse_context;
use issue_body::render_issue_body;
use options::Options;
use schema::{build_schema_report, SchemaMonitorInput};

const MARKER: &str = "<!-- rmcp-release-monitor -->";
const DEFAULT_MAX_BODY_BYTES: usize = 60_000;

#[derive(Debug)]
struct MonitorReport {
    drift: bool,
    rmcp_drift: bool,
    mcp_schema_drift: bool,
    conformance_drift: bool,
    current_version: String,
    latest_version: String,
    issue_title: String,
    issue_body: String,
}

#[derive(Debug, Deserialize)]
struct CratesIoResponse {
    #[serde(rename = "crate")]
    crate_info: CrateInfo,
    versions: Vec<CrateVersion>,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    max_version: String,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    documentation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrateVersion {
    num: String,
    created_at: String,
    yanked: bool,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: Option<String>,
    published_at: Option<String>,
    body: Option<String>,
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    let options = Options::parse(args)?;
    let current_version = match &options.current_version {
        Some(version) => version.clone(),
        None => detect_current_rmcp_version(Path::new("."))?,
    };
    let crate_json = fs::read_to_string(&options.crate_json)
        .with_context(|| format!("failed to read {}", options.crate_json.display()))?;
    let releases_json = fs::read_to_string(&options.releases_json)
        .with_context(|| format!("failed to read {}", options.releases_json.display()))?;
    let schema = options.schema_input()?;
    let conformance = options.conformance_input()?;
    let report = build_monitor_report(
        &current_version,
        &crate_json,
        &releases_json,
        schema.as_ref(),
        conformance.as_ref(),
        options.max_body_bytes,
    )?;

    if let Some(parent) = options.issue_body.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&options.issue_body, &report.issue_body)
        .with_context(|| format!("failed to write {}", options.issue_body.display()))?;

    println!("drift={}", report.drift);
    println!("rmcp_drift={}", report.rmcp_drift);
    println!("mcp_schema_drift={}", report.mcp_schema_drift);
    println!("conformance_drift={}", report.conformance_drift);
    println!("current_version={}", report.current_version);
    println!("latest_version={}", report.latest_version);
    println!("issue_title={}", report.issue_title);
    write_github_output("drift", if report.drift { "true" } else { "false" })?;
    write_github_output(
        "rmcp_drift",
        if report.rmcp_drift { "true" } else { "false" },
    )?;
    write_github_output(
        "mcp_schema_drift",
        if report.mcp_schema_drift {
            "true"
        } else {
            "false"
        },
    )?;
    write_github_output(
        "conformance_drift",
        if report.conformance_drift {
            "true"
        } else {
            "false"
        },
    )?;
    write_github_output("current_version", &report.current_version)?;
    write_github_output("latest_version", &report.latest_version)?;
    write_github_output("issue_title", &report.issue_title)?;
    Ok(())
}

fn build_monitor_report(
    current_version: &str,
    crate_json: &str,
    releases_json: &str,
    schema: Option<&SchemaMonitorInput>,
    conformance: Option<&ConformanceMonitorInput>,
    max_body_bytes: usize,
) -> Result<MonitorReport> {
    let metadata: CratesIoResponse = serde_json::from_str(crate_json).with_context(|| {
        json_parse_context("crates.io rmcp metadata (--crate-json)", crate_json)
    })?;
    let releases: Vec<GithubRelease> = serde_json::from_str(releases_json).with_context(|| {
        json_parse_context("GitHub release metadata (--releases-json)", releases_json)
    })?;
    let current = Version::parse(exact_version(current_version))
        .with_context(|| format!("invalid current rmcp version {current_version:?}"))?;
    let latest = latest_non_yanked_version(&metadata)?;
    let rmcp_drift = latest > current;
    let schema_report = schema.map(build_schema_report).transpose()?;
    let mcp_schema_drift = schema_report.as_ref().is_some_and(|report| report.drift);
    let conformance_report = conformance.map(build_conformance_report).transpose()?;
    let conformance_drift = conformance_report
        .as_ref()
        .is_some_and(|report| report.drift);
    let drift = rmcp_drift || mcp_schema_drift || conformance_drift;
    let latest_version = latest.to_string();
    let issue_title = match (rmcp_drift, mcp_schema_drift, conformance_drift) {
        (true, false, false) => {
            format!("rmcp {latest_version} released (Soma pins {current_version})")
        }
        (false, true, false) => "MCP schema changed upstream".to_owned(),
        (false, false, true) => "MCP conformance changed upstream".to_owned(),
        (false, false, false) => {
            format!("rmcp, MCP schema, and conformance are current at {current_version}")
        }
        _ => "MCP upstream changes need Soma review".to_owned(),
    };
    let issue_body = if drift {
        render_issue_body(
            &metadata,
            &releases,
            &current,
            &latest,
            schema_report.as_ref(),
            conformance_report.as_ref(),
            max_body_bytes,
        )?
    } else {
        format!(
            "{MARKER}\n<!-- rmcp-current-version: {current_version} -->\n<!-- rmcp-latest-version: {latest_version} -->\n\nThe Soma rmcp pin, MCP schema baseline, and conformance baseline are current.\n"
        )
    };
    Ok(MonitorReport {
        drift,
        rmcp_drift,
        mcp_schema_drift,
        conformance_drift,
        current_version: current_version.to_owned(),
        latest_version,
        issue_title,
        issue_body,
    })
}

/// Strips the comparator from a cargo version requirement so it can be parsed
/// as a concrete `Version`.
///
/// The workspace pins rmcp exactly (`rmcp = { version = "=3.0.0-beta.2" }`),
/// and `detect_current_rmcp_version` returns that requirement verbatim.
/// `semver::Version::parse` rejects the leading `=`, so the monitor could
/// never read an exactly-pinned workspace.
fn exact_version(requirement: &str) -> &str {
    requirement
        .trim()
        .trim_start_matches(['=', '^', '~', 'v'])
        .trim()
}

fn detect_current_rmcp_version(root: &Path) -> Result<String> {
    let manifest_versions = discover_rmcp_manifest_versions(root)?;
    let versions: BTreeSet<_> = manifest_versions
        .iter()
        .map(|(_, version)| version.clone())
        .collect();
    match versions.len() {
        0 => bail!("no rmcp dependency version found in workspace manifests"),
        1 => Ok(versions.into_iter().next().expect("one version")),
        _ => bail!(
            "conflicting rmcp versions across workspace manifests: {}",
            manifest_versions
                .iter()
                .map(|(path, version)| format!("{}={version}", path.display()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn discover_rmcp_manifest_versions(root: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut manifest_versions = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !is_ignored_manifest_dir(entry.path()))
    {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        if !entry.file_type().is_file() || entry.file_name() != "Cargo.toml" {
            continue;
        }
        let path = entry.path();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if let Some(version) = rmcp_version_from_manifest(&text) {
            let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
            manifest_versions.push((relative, version));
        }
    }
    manifest_versions.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(manifest_versions)
}

fn is_ignored_manifest_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, ".git" | ".worktrees" | "target" | "node_modules"))
}

fn rmcp_version_from_manifest(text: &str) -> Option<String> {
    text.lines().find_map(|raw_line| {
        let line = raw_line.trim();
        if line.starts_with('#') || !line.starts_with("rmcp") {
            return None;
        }
        let (name, rhs) = line.split_once('=')?;
        if name.trim() != "rmcp" {
            return None;
        }
        quoted_version(rhs)
    })
}

fn quoted_version(value: &str) -> Option<String> {
    if let Some(rest) = value.trim().strip_prefix('"') {
        return rest.split_once('"').map(|(version, _)| version.to_owned());
    }
    let (_, after_version) = value.split_once("version")?;
    let (_, after_equals) = after_version.split_once('=')?;
    let rest = after_equals.trim().strip_prefix('"')?;
    rest.split_once('"').map(|(version, _)| version.to_owned())
}

fn latest_non_yanked_version(metadata: &CratesIoResponse) -> Result<Version> {
    let mut latest = Version::parse(&metadata.crate_info.max_version).with_context(|| {
        format!(
            "invalid max rmcp version {:?}",
            metadata.crate_info.max_version
        )
    })?;
    if metadata
        .versions
        .iter()
        .any(|version| !version.yanked && version.num == latest.to_string())
    {
        return Ok(latest);
    }
    latest = metadata
        .versions
        .iter()
        .filter(|version| !version.yanked)
        .filter_map(|version| Version::parse(&version.num).ok())
        .max()
        .context("crates.io metadata did not contain any non-yanked rmcp versions")?;
    Ok(latest)
}

/// Twelve hex digits is what GitHub renders and what a reviewer scans;
/// upstream payloads carry the full 40.
fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

fn write_github_output(key: &str, value: &str) -> Result<()> {
    let Some(path) = std::env::var_os("GITHUB_OUTPUT").map(PathBuf::from) else {
        return Ok(());
    };
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{key}={value}")?;
    Ok(())
}

/// The `_tests.rs` sibling is a real module, not just a file that satisfies
/// `cargo xtask check-test-siblings`. It was previously unreferenced, so
/// everything in it silently never ran.
#[cfg(test)]
#[path = "rmcp_release_monitor_tests.rs"]
mod sibling_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const CRATE_JSON: &str = r#"{
      "crate": {
        "name": "rmcp",
        "max_version": "1.8.0",
        "repository": "https://github.com/modelcontextprotocol/rust-sdk/",
        "homepage": "https://github.com/modelcontextprotocol/rust-sdk",
        "documentation": "https://docs.rs/rmcp"
      },
      "versions": [
        {"num": "1.8.0", "created_at": "2026-06-23T12:28:57.399938Z", "yanked": false},
        {"num": "1.7.0", "created_at": "2026-05-13T13:44:43.260847Z", "yanked": false}
      ]
    }"#;

    const RELEASES_JSON: &str = r#"[
      {
        "tag_name": "rmcp-v1.8.0",
        "name": "rmcp-v1.8.0",
        "html_url": "https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v1.8.0",
        "published_at": "2026-06-23T12:29:09Z",
        "body": "> [!WARNING]\n> Breaking Changes\n\nPeer::peer_info() return type changed.\n\n### Fixed\n- strip and validate tool outputSchema and inputSchema"
      },
      {
        "tag_name": "rmcp-v1.7.0",
        "name": "rmcp-v1.7.0",
        "html_url": "https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v1.7.0",
        "published_at": "2026-05-13T13:44:49Z",
        "body": "already pinned"
      }
    ]"#;

    const COMMITS_JSON: &str = r#"[
      {
        "sha": "357adac47ab2654b64799f994e6db8d3df4ee19d",
        "html_url": "https://github.com/modelcontextprotocol/modelcontextprotocol/commit/357adac47ab2654b64799f994e6db8d3df4ee19d",
        "commit": {
          "message": "schema: allow null for Task.ttl in generated JSON schema\n\nbody",
          "author": {"date": "2026-03-15T17:36:29Z"}
        }
      }
    ]"#;

    const CONFORMANCE_HEAD_JSON: &str = r#"{
      "sha": "32523cc21a344373408c622c772ba09866e58158",
      "html_url": "https://github.com/modelcontextprotocol/conformance/commit/32523cc21a344373408c622c772ba09866e58158",
      "commit": {
        "message": "feat: CIMD support check for authorization-server metadata\n\nbody",
        "author": {"date": "2026-06-24T15:53:00Z"}
      }
    }"#;

    const CONFORMANCE_COMPARE_JSON: &str = r#"{
      "commits": [
        {
          "sha": "32523cc21a344373408c622c772ba09866e58158",
          "html_url": "https://github.com/modelcontextprotocol/conformance/commit/32523cc21a344373408c622c772ba09866e58158",
          "commit": {
            "message": "feat: CIMD support check for authorization-server metadata\n\nbody",
            "author": {"date": "2026-06-24T15:53:00Z"}
          }
        }
      ],
      "files": [
        {
          "filename": "src/scenarios/authorization-server/authorization-server-metadata.ts",
          "status": "modified",
          "additions": 39,
          "deletions": 3,
          "changes": 42,
          "blob_url": "https://github.com/modelcontextprotocol/conformance/blob/32523cc/src/scenarios/authorization-server/authorization-server-metadata.ts",
          "patch": "+ id: 'authorization-server-metadata-cimd'\n+ client_id_metadata_document_supported: true\n"
        }
      ]
    }"#;

    #[test]
    fn report_detects_new_rmcp_release_and_includes_release_notes() {
        let report = build_monitor_report("1.7.0", CRATE_JSON, RELEASES_JSON, None, None, 60_000)
            .expect("monitor report");

        assert!(report.drift);
        assert!(report.rmcp_drift);
        assert!(!report.mcp_schema_drift);
        assert!(!report.conformance_drift);
        assert_eq!(report.current_version, "1.7.0");
        assert_eq!(report.latest_version, "1.8.0");
        assert!(report.issue_title.contains("rmcp 1.8.0 released"));
        assert!(report.issue_body.contains("<!-- rmcp-release-monitor -->"));
        assert!(
            report
                .issue_body
                .contains("<!-- rmcp-latest-version: 1.8.0 -->")
        );
        assert!(
            report
                .issue_body
                .contains("Peer::peer_info() return type changed")
        );
        assert!(
            report
                .issue_body
                .contains("strip and validate tool outputSchema")
        );
        assert!(report.issue_body.contains(
            "https://github.com/modelcontextprotocol/rust-sdk/compare/rmcp-v1.7.0...rmcp-v1.8.0"
        ));
    }

    #[test]
    fn report_includes_mcp_schema_drift_when_schema_hash_changes() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("crates/soma/mcp/src")).unwrap();
        fs::write(
            temp.path().join("crates/soma/mcp/src/rmcp_server.rs"),
            "fn inspect_schema() { let _schema_type = \"NewThing\"; }\n",
        )
        .unwrap();
        let schema = SchemaMonitorInput {
            baseline: "export const LATEST_PROTOCOL_VERSION = \"2025-11-25\";\n".to_owned(),
            upstream: "export const LATEST_PROTOCOL_VERSION = \"2025-11-25\";\nexport interface NewThing {}\n".to_owned(),
            commits_json: Some(COMMITS_JSON.to_owned()),
            url: "https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/schema/2025-11-25/schema.ts".to_owned(),
            repo_root: temp.path().to_path_buf(),
        };

        let report = build_monitor_report(
            "1.8.0",
            CRATE_JSON,
            RELEASES_JSON,
            Some(&schema),
            None,
            60_000,
        )
        .expect("monitor report");

        assert!(report.drift);
        assert!(!report.rmcp_drift);
        assert!(report.mcp_schema_drift);
        assert!(!report.conformance_drift);
        assert_eq!(report.issue_title, "MCP schema changed upstream");
        assert!(report.issue_body.contains("## MCP Schema Watch"));
        assert!(report.issue_body.contains("mcp-schema-baseline-sha256"));
        assert!(report.issue_body.contains("mcp-schema-upstream-sha256"));
        assert!(
            report
                .issue_body
                .contains("schema: allow null for Task.ttl")
        );
        assert!(
            report
                .issue_body
                .contains("Potential schema impact in this repo")
        );
        assert!(
            report
                .issue_body
                .contains("crates/soma/mcp/src/rmcp_server.rs")
        );
        assert!(report.issue_body.contains("`NewThing`"));
        assert!(report.issue_body.contains("+export interface NewThing {}"));
    }

    #[test]
    fn matching_mcp_schema_hash_does_not_create_drift_by_itself() {
        let temp = TempDir::new().unwrap();
        let schema = SchemaMonitorInput {
            baseline: "same schema\n".to_owned(),
            upstream: "same schema\n".to_owned(),
            commits_json: None,
            url: "https://example.test/schema.ts".to_owned(),
            repo_root: temp.path().to_path_buf(),
        };

        let report = build_monitor_report(
            "1.8.0",
            CRATE_JSON,
            RELEASES_JSON,
            Some(&schema),
            None,
            60_000,
        )
        .expect("monitor report");

        assert!(!report.drift);
        assert!(!report.rmcp_drift);
        assert!(!report.mcp_schema_drift);
        assert!(!report.conformance_drift);
    }

    #[test]
    fn report_includes_conformance_drift_and_repo_impact_candidates() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("crates/soma/runtime/src")).unwrap();
        fs::write(
            temp.path().join("crates/soma/runtime/src/server.rs"),
            "const AUTH_METADATA_FIELD: &str = \"client_id_metadata_document_supported\";\n",
        )
        .unwrap();
        let conformance = ConformanceMonitorInput {
            baseline_sha: "565eaffc902017060cb8bc38517af7de0f2e2adb\n".to_owned(),
            head_json: CONFORMANCE_HEAD_JSON.to_owned(),
            compare_json: Some(CONFORMANCE_COMPARE_JSON.to_owned()),
            url: "https://github.com/modelcontextprotocol/conformance".to_owned(),
            repo_root: temp.path().to_path_buf(),
        };

        let report = build_monitor_report(
            "1.8.0",
            CRATE_JSON,
            RELEASES_JSON,
            None,
            Some(&conformance),
            60_000,
        )
        .expect("monitor report");

        assert!(report.drift);
        assert!(!report.rmcp_drift);
        assert!(!report.mcp_schema_drift);
        assert!(report.conformance_drift);
        assert_eq!(report.issue_title, "MCP conformance changed upstream");
        assert!(report.issue_body.contains("## MCP Conformance Watch"));
        assert!(report.issue_body.contains("mcp-conformance-baseline-sha"));
        assert!(report.issue_body.contains("feat: CIMD support check"));
        assert!(
            report
                .issue_body
                .contains("authorization-server-metadata.ts")
        );
        assert!(
            report
                .issue_body
                .contains("Potential conformance impact in this repo")
        );
        assert!(
            report
                .issue_body
                .contains("crates/soma/runtime/src/server.rs")
        );
        assert!(
            report
                .issue_body
                .contains("`client_id_metadata_document_supported`")
        );
    }

    #[test]
    fn current_version_discovery_requires_consistent_rmcp_pins() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        for crate_path in [
            "apps/soma",
            "crates/shared/auth",
            "crates/soma/mcp",
            "crates/shared/traces",
        ] {
            fs::create_dir_all(root.join(crate_path)).unwrap();
            fs::write(
                root.join(format!("{crate_path}/Cargo.toml")),
                "rmcp = { version = \"1.7.0\", default-features = false }\n",
            )
            .unwrap();
        }
        fs::create_dir_all(root.join("crates/no-rmcp")).unwrap();
        fs::write(
            root.join("crates/no-rmcp/Cargo.toml"),
            "[package]\nname = \"no-rmcp\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".worktrees/stale/crates/stale")).unwrap();
        fs::write(
            root.join(".worktrees/stale/crates/stale/Cargo.toml"),
            "rmcp = { version = \"9.9.9\", default-features = false }\n",
        )
        .unwrap();

        assert_eq!(detect_current_rmcp_version(root).unwrap(), "1.7.0");

        fs::write(
            root.join("crates/shared/traces/Cargo.toml"),
            "rmcp = { version = \"1.8.0\", default-features = false }\n",
        )
        .unwrap();
        let error = detect_current_rmcp_version(root).expect_err("mixed pins should fail");
        let message = error.to_string();
        let normalized_message = message.replace('\\', "/");
        assert!(message.contains("conflicting rmcp versions"));
        assert!(normalized_message.contains("crates/shared/traces/Cargo.toml=1.8.0"));
        assert!(!message.contains("9.9.9"));
    }

    #[test]
    fn workflow_uses_hidden_marker_and_stable_issue_update_path() {
        let workflow = include_str!("../../.github/workflows/rmcp-release-monitor.yml");

        assert!(workflow.contains("rmcp-release-monitor in:body"));
        assert!(workflow.contains("gh issue edit"));
        assert!(workflow.contains("gh issue create"));
        assert!(workflow.contains("cargo xtask rmcp-release-monitor"));
        assert!(workflow.contains("--schema-baseline"));
        assert!(workflow.contains("--schema-upstream"));
        assert!(workflow.contains("schema/2025-11-25/schema.ts"));
        assert!(workflow.contains("--conformance-baseline"));
        assert!(workflow.contains("--conformance-head-json"));
        assert!(workflow.contains("modelcontextprotocol/conformance"));
        assert!(workflow.contains("issues: write"));
    }

    #[test]
    fn issue_body_truncation_preserves_utf8_boundary() {
        let body = format!("{}{}", "a".repeat(200), "⚠️".repeat(10));
        let truncated = super::issue_body::clamp_issue_body(body, 230);

        assert!(truncated.contains("rmcp-release-monitor-truncated"));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }
}
