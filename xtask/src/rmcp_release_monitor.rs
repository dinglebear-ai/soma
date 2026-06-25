use anyhow::{bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MARKER: &str = "<!-- rmcp-release-monitor -->";
const DEFAULT_MAX_BODY_BYTES: usize = 60_000;
const RMCP_MANIFESTS: [&str; 2] = [
    "crates/rmcp-template/Cargo.toml",
    "crates/rtemplate-mcp/Cargo.toml",
];

#[derive(Debug)]
struct MonitorReport {
    drift: bool,
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

#[derive(Debug)]
struct Options {
    crate_json: PathBuf,
    releases_json: PathBuf,
    issue_body: PathBuf,
    current_version: Option<String>,
    max_body_bytes: usize,
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
    let report = build_monitor_report(
        &current_version,
        &crate_json,
        &releases_json,
        options.max_body_bytes,
    )?;

    if let Some(parent) = options.issue_body.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    fs::write(&options.issue_body, &report.issue_body)
        .with_context(|| format!("failed to write {}", options.issue_body.display()))?;

    println!("drift={}", report.drift);
    println!("current_version={}", report.current_version);
    println!("latest_version={}", report.latest_version);
    println!("issue_title={}", report.issue_title);
    write_github_output("drift", if report.drift { "true" } else { "false" })?;
    write_github_output("current_version", &report.current_version)?;
    write_github_output("latest_version", &report.latest_version)?;
    write_github_output("issue_title", &report.issue_title)?;
    Ok(())
}

fn build_monitor_report(
    current_version: &str,
    crate_json: &str,
    releases_json: &str,
    max_body_bytes: usize,
) -> Result<MonitorReport> {
    let metadata: CratesIoResponse =
        serde_json::from_str(crate_json).context("failed to parse crates.io rmcp metadata")?;
    let releases: Vec<GithubRelease> =
        serde_json::from_str(releases_json).context("failed to parse GitHub release metadata")?;
    let current = Version::parse(current_version)
        .with_context(|| format!("invalid current rmcp version {current_version:?}"))?;
    let latest = latest_non_yanked_version(&metadata)?;
    let drift = latest > current;
    let latest_version = latest.to_string();
    let issue_title = if drift {
        format!("rmcp {latest_version} released (template pins {current_version})")
    } else {
        format!("rmcp is current at {current_version}")
    };
    let issue_body = if drift {
        render_issue_body(&metadata, &releases, &current, &latest, max_body_bytes)?
    } else {
        format!(
            "{MARKER}\n<!-- rmcp-current-version: {current_version} -->\n<!-- rmcp-latest-version: {latest_version} -->\n\nThe template rmcp pin is current.\n"
        )
    };
    Ok(MonitorReport {
        drift,
        current_version: current_version.to_owned(),
        latest_version,
        issue_title,
        issue_body,
    })
}

fn detect_current_rmcp_version(root: &Path) -> Result<String> {
    let mut versions = BTreeSet::new();
    for manifest in RMCP_MANIFESTS {
        let path = root.join(manifest);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if let Some(version) = rmcp_version_from_manifest(&text) {
            versions.insert(version);
        }
    }
    match versions.len() {
        0 => bail!("no rmcp dependency version found in tracked manifests"),
        1 => Ok(versions.into_iter().next().expect("one version")),
        _ => bail!("conflicting rmcp versions across manifests: {versions:?}"),
    }
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

fn render_issue_body(
    metadata: &CratesIoResponse,
    releases: &[GithubRelease],
    current: &Version,
    latest: &Version,
    max_body_bytes: usize,
) -> Result<String> {
    let released_versions = released_versions_between(metadata, current, latest);
    let repository = metadata
        .crate_info
        .repository
        .as_deref()
        .or(metadata.crate_info.homepage.as_deref());
    let compare_url = repository.and_then(|repo| github_compare_url(repo, current, latest));

    let mut body = String::new();
    body.push_str(MARKER);
    body.push('\n');
    body.push_str(&format!("<!-- rmcp-current-version: {current} -->\n"));
    body.push_str(&format!("<!-- rmcp-latest-version: {latest} -->\n\n"));
    body.push_str(&format!(
        "`rmcp` has a newer published crate release. This template currently pins `{current}` and crates.io now publishes `{latest}`.\n\n"
    ));
    body.push_str("## Release Window\n\n");
    body.push_str("| Version | Published | Yanked | Links |\n");
    body.push_str("|---|---:|:---:|---|\n");
    for version in &released_versions {
        let release = find_release(releases, &version.num);
        let release_link = release
            .and_then(|release| release.html_url.as_deref())
            .map(|url| format!(" [release]({url})"))
            .unwrap_or_default();
        body.push_str(&format!(
            "| `{}` | `{}` | {} | [crates.io](https://crates.io/crates/rmcp/{}){} |\n",
            version.num,
            version.created_at,
            if version.yanked { "yes" } else { "no" },
            version.num,
            release_link
        ));
    }
    body.push('\n');
    body.push_str("## Review Links\n\n");
    body.push_str("- [rmcp on crates.io](https://crates.io/crates/rmcp)\n");
    if let Some(docs) = &metadata.crate_info.documentation {
        body.push_str(&format!("- [docs.rs]({docs})\n"));
    }
    if let Some(repo) = repository {
        body.push_str(&format!("- [upstream repository]({repo})\n"));
    }
    if let Some(url) = compare_url {
        body.push_str(&format!("- [upstream compare]({url})\n"));
    }
    body.push('\n');
    body.push_str("## Release Notes\n\n");
    for version in &released_versions {
        let release = find_release(releases, &version.num);
        body.push_str(&format!("### rmcp v{}\n\n", version.num));
        if let Some(release) = release {
            if let Some(published_at) = &release.published_at {
                body.push_str(&format!("Published: `{published_at}`\n\n"));
            }
            if let Some(name) = &release.name {
                body.push_str(&format!("Release: `{name}`\n\n"));
            }
            let notes = release.body.as_deref().unwrap_or("").trim();
            if notes.is_empty() {
                body.push_str("_No GitHub release notes were published for this tag._\n\n");
            } else {
                body.push_str(notes);
                body.push_str("\n\n");
            }
        } else {
            body.push_str("_No matching GitHub release was found for this crate version._\n\n");
        }
    }
    body.push_str("## Suggested Follow-Up\n\n");
    body.push_str("- Read the release notes above for source-breaking changes.\n");
    body.push_str("- Update all `rmcp` pins together.\n");
    body.push_str(
        "- Run `cargo update -p rmcp`, `cargo test`, and the MCP dispatch/schema checks.\n",
    );
    body.push_str("- Update template docs/examples if the rmcp API or feature flags changed.\n");
    Ok(clamp_issue_body(body, max_body_bytes))
}

fn released_versions_between<'a>(
    metadata: &'a CratesIoResponse,
    current: &Version,
    latest: &Version,
) -> Vec<&'a CrateVersion> {
    let mut versions = metadata
        .versions
        .iter()
        .filter(|version| !version.yanked)
        .filter(|version| {
            Version::parse(&version.num)
                .map(|parsed| parsed > *current && parsed <= *latest)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| {
        Version::parse(&left.num)
            .unwrap_or_else(|_| Version::new(0, 0, 0))
            .cmp(&Version::parse(&right.num).unwrap_or_else(|_| Version::new(0, 0, 0)))
    });
    versions
}

fn find_release<'a>(releases: &'a [GithubRelease], version: &str) -> Option<&'a GithubRelease> {
    let tag = format!("rmcp-v{version}");
    releases.iter().find(|release| release.tag_name == tag)
}

fn github_compare_url(repo: &str, current: &Version, latest: &Version) -> Option<String> {
    let trimmed = repo.trim_end_matches('/').trim_end_matches(".git");
    let path = trimmed.strip_prefix("https://github.com/")?;
    Some(format!(
        "https://github.com/{path}/compare/rmcp-v{current}...rmcp-v{latest}"
    ))
}

fn clamp_issue_body(mut body: String, max_body_bytes: usize) -> String {
    let marker = "\n\n<!-- rmcp-release-monitor-truncated: true -->\n\n_Release notes were truncated to keep this issue body under GitHub's size limit. Use the release and compare links above for the full upstream changes._\n";
    if body.len() <= max_body_bytes || max_body_bytes <= marker.len() {
        return body;
    }
    let mut keep_bytes = max_body_bytes - marker.len();
    while !body.is_char_boundary(keep_bytes) {
        keep_bytes = keep_bytes.saturating_sub(1);
    }
    body.truncate(keep_bytes);
    body.push_str(marker);
    body
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

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut crate_json = None;
        let mut releases_json = None;
        let mut issue_body = None;
        let mut current_version = None;
        let mut max_body_bytes = DEFAULT_MAX_BODY_BYTES;
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--crate-json" => {
                    index += 1;
                    crate_json = Some(PathBuf::from(value_arg(args, index, "--crate-json")?));
                }
                "--releases-json" => {
                    index += 1;
                    releases_json = Some(PathBuf::from(value_arg(args, index, "--releases-json")?));
                }
                "--issue-body" => {
                    index += 1;
                    issue_body = Some(PathBuf::from(value_arg(args, index, "--issue-body")?));
                }
                "--current-version" => {
                    index += 1;
                    current_version = Some(value_arg(args, index, "--current-version")?.to_owned());
                }
                "--max-body-bytes" => {
                    index += 1;
                    max_body_bytes = value_arg(args, index, "--max-body-bytes")?
                        .parse::<usize>()
                        .context("--max-body-bytes must be an integer")?;
                }
                "--help" | "-h" => bail!(
                    "Usage: cargo xtask rmcp-release-monitor --crate-json rmcp.json --releases-json releases.json --issue-body issue.md [--current-version VERSION] [--max-body-bytes N]"
                ),
                unknown => bail!("unknown rmcp-release-monitor option: {unknown}"),
            }
            index += 1;
        }
        Ok(Self {
            crate_json: crate_json.context("--crate-json is required")?,
            releases_json: releases_json.context("--releases-json is required")?,
            issue_body: issue_body.context("--issue-body is required")?,
            current_version,
            max_body_bytes,
        })
    }
}

fn value_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .with_context(|| format!("{flag} requires a value"))
}

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

    #[test]
    fn report_detects_new_rmcp_release_and_includes_release_notes() {
        let report = build_monitor_report("1.7.0", CRATE_JSON, RELEASES_JSON, 60_000)
            .expect("monitor report");

        assert!(report.drift);
        assert_eq!(report.current_version, "1.7.0");
        assert_eq!(report.latest_version, "1.8.0");
        assert!(report.issue_title.contains("rmcp 1.8.0 released"));
        assert!(report.issue_body.contains("<!-- rmcp-release-monitor -->"));
        assert!(report
            .issue_body
            .contains("<!-- rmcp-latest-version: 1.8.0 -->"));
        assert!(report
            .issue_body
            .contains("Peer::peer_info() return type changed"));
        assert!(report
            .issue_body
            .contains("strip and validate tool outputSchema"));
        assert!(report.issue_body.contains(
            "https://github.com/modelcontextprotocol/rust-sdk/compare/rmcp-v1.7.0...rmcp-v1.8.0"
        ));
    }

    #[test]
    fn current_version_discovery_requires_consistent_rmcp_pins() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("crates/rmcp-template")).unwrap();
        fs::create_dir_all(root.join("crates/rtemplate-mcp")).unwrap();
        fs::write(
            root.join("crates/rmcp-template/Cargo.toml"),
            "rmcp = { version = \"1.7.0\", default-features = false }\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/rtemplate-mcp/Cargo.toml"),
            "rmcp = { version = \"1.7.0\", default-features = false }\n",
        )
        .unwrap();

        assert_eq!(detect_current_rmcp_version(root).unwrap(), "1.7.0");

        fs::write(
            root.join("crates/rtemplate-mcp/Cargo.toml"),
            "rmcp = { version = \"1.8.0\", default-features = false }\n",
        )
        .unwrap();
        let error = detect_current_rmcp_version(root).expect_err("mixed pins should fail");
        assert!(error.to_string().contains("conflicting rmcp versions"));
    }

    #[test]
    fn workflow_uses_hidden_marker_and_stable_issue_update_path() {
        let workflow = include_str!("../../.github/workflows/rmcp-release-monitor.yml");

        assert!(workflow.contains("rmcp-release-monitor in:body"));
        assert!(workflow.contains("gh issue edit"));
        assert!(workflow.contains("gh issue create"));
        assert!(workflow.contains("cargo xtask rmcp-release-monitor"));
        assert!(workflow.contains("issues: write"));
    }

    #[test]
    fn issue_body_truncation_preserves_utf8_boundary() {
        let body = format!("{}{}", "a".repeat(200), "⚠️".repeat(10));
        let truncated = clamp_issue_body(body, 230);

        assert!(truncated.contains("rmcp-release-monitor-truncated"));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }
}
