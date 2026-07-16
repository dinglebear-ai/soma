use anyhow::{bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

#[derive(Debug)]
struct Options {
    crate_json: PathBuf,
    releases_json: PathBuf,
    issue_body: PathBuf,
    schema_baseline: Option<PathBuf>,
    schema_upstream: Option<PathBuf>,
    schema_commits_json: Option<PathBuf>,
    schema_url: String,
    conformance_baseline: Option<PathBuf>,
    conformance_head_json: Option<PathBuf>,
    conformance_compare_json: Option<PathBuf>,
    conformance_url: String,
    current_version: Option<String>,
    max_body_bytes: usize,
}

#[derive(Debug)]
struct SchemaMonitorInput {
    baseline: String,
    upstream: String,
    commits_json: Option<String>,
    url: String,
    repo_root: PathBuf,
}

#[derive(Debug)]
struct SchemaReport {
    drift: bool,
    baseline_hash: String,
    upstream_hash: String,
    url: String,
    diff: String,
    commits: Vec<SchemaCommit>,
    impacts: Vec<RepoImpact>,
}

#[derive(Debug)]
struct ConformanceMonitorInput {
    baseline_sha: String,
    head_json: String,
    compare_json: Option<String>,
    url: String,
    repo_root: PathBuf,
}

#[derive(Debug)]
struct ConformanceReport {
    drift: bool,
    baseline_sha: String,
    head_sha: String,
    url: String,
    head_date: String,
    head_message: String,
    head_html_url: String,
    commits: Vec<ConformanceCommit>,
    files: Vec<ConformanceFile>,
    impacts: Vec<RepoImpact>,
}

#[derive(Debug)]
struct RepoImpact {
    path: String,
    identifiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SchemaCommit {
    sha: String,
    html_url: String,
    commit: SchemaCommitDetails,
}

#[derive(Debug, Deserialize)]
struct SchemaCommitDetails {
    message: String,
    author: SchemaCommitAuthor,
}

#[derive(Debug, Deserialize)]
struct SchemaCommitAuthor {
    date: String,
}

#[derive(Debug, Deserialize)]
struct ConformanceHead {
    sha: String,
    html_url: String,
    commit: ConformanceCommitDetails,
}

#[derive(Debug, Deserialize)]
struct ConformanceCompare {
    #[serde(default)]
    commits: Vec<ConformanceCommit>,
    #[serde(default)]
    files: Vec<ConformanceFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct ConformanceCommit {
    sha: String,
    html_url: String,
    commit: ConformanceCommitDetails,
}

#[derive(Debug, Clone, Deserialize)]
struct ConformanceCommitDetails {
    message: String,
    author: ConformanceCommitAuthor,
}

#[derive(Debug, Clone, Deserialize)]
struct ConformanceCommitAuthor {
    date: String,
}

#[derive(Debug, Deserialize)]
struct ConformanceFile {
    filename: String,
    status: String,
    additions: u64,
    deletions: u64,
    changes: u64,
    #[serde(default)]
    blob_url: Option<String>,
    #[serde(default)]
    patch: Option<String>,
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

    if let Some(parent) = options.issue_body.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
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
    let metadata: CratesIoResponse =
        serde_json::from_str(crate_json).context("failed to parse crates.io rmcp metadata")?;
    let releases: Vec<GithubRelease> =
        serde_json::from_str(releases_json).context("failed to parse GitHub release metadata")?;
    let current = Version::parse(current_version)
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

fn render_issue_body(
    metadata: &CratesIoResponse,
    releases: &[GithubRelease],
    current: &Version,
    latest: &Version,
    schema_report: Option<&SchemaReport>,
    conformance_report: Option<&ConformanceReport>,
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
    if let Some(report) = schema_report {
        body.push_str(&format!(
            "<!-- mcp-schema-baseline-sha256: {} -->\n",
            report.baseline_hash
        ));
        body.push_str(&format!(
            "<!-- mcp-schema-upstream-sha256: {} -->\n",
            report.upstream_hash
        ));
    }
    if let Some(report) = conformance_report {
        body.push_str(&format!(
            "<!-- mcp-conformance-baseline-sha: {} -->\n",
            report.baseline_sha
        ));
        body.push_str(&format!(
            "<!-- mcp-conformance-head-sha: {} -->\n",
            report.head_sha
        ));
    }
    body.push('\n');
    if latest > current {
        body.push_str(&format!(
            "`rmcp` has a newer published crate release. Soma currently pins `{current}` and crates.io now publishes `{latest}`.\n\n"
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
    }
    if let Some(report) = schema_report {
        append_schema_section(&mut body, report);
    }
    if let Some(report) = conformance_report {
        append_conformance_section(&mut body, report);
    }
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
    if latest > current {
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
    }
    body.push_str("## Suggested Follow-Up\n\n");
    body.push_str(
        "- Read the release, schema, and conformance sections above for source-breaking changes.\n",
    );
    body.push_str("- Update all `rmcp` pins together when rmcp drift is present.\n");
    body.push_str("- Refresh the pinned MCP schema baseline after reviewing schema drift.\n");
    body.push_str(
        "- Refresh the pinned MCP conformance baseline after reviewing conformance drift.\n",
    );
    body.push_str(
        "- Run `cargo update -p rmcp`, `cargo test`, and the MCP dispatch/schema/conformance checks.\n",
    );
    body.push_str("- Update Soma docs/examples if the rmcp API or feature flags changed.\n");
    Ok(clamp_issue_body(body, max_body_bytes))
}

fn build_schema_report(input: &SchemaMonitorInput) -> Result<SchemaReport> {
    let baseline_hash = sha256_hex(input.baseline.as_bytes());
    let upstream_hash = sha256_hex(input.upstream.as_bytes());
    let drift = baseline_hash != upstream_hash;
    let commits = input
        .commits_json
        .as_deref()
        .map(|json| serde_json::from_str(json).context("failed to parse MCP schema commit JSON"))
        .transpose()?
        .unwrap_or_default();
    let changed_terms = if drift {
        changed_terms_from_text_diff(&input.baseline, &input.upstream)
    } else {
        BTreeSet::new()
    };
    Ok(SchemaReport {
        drift,
        baseline_hash,
        upstream_hash,
        url: input.url.clone(),
        diff: if drift {
            simple_unified_diff(
                "docs/references/mcp/schema/2025-11-25/schema.ts",
                &input.url,
                &input.baseline,
                &input.upstream,
                30_000,
            )
        } else {
            String::new()
        },
        commits,
        impacts: if drift {
            scan_repo_impacts(&input.repo_root, &changed_terms)?
        } else {
            Vec::new()
        },
    })
}

fn build_conformance_report(input: &ConformanceMonitorInput) -> Result<ConformanceReport> {
    let head: ConformanceHead = serde_json::from_str(&input.head_json)
        .context("failed to parse MCP conformance head JSON")?;
    let baseline_sha = input.baseline_sha.trim().to_owned();
    let drift = baseline_sha != head.sha;
    let compare = input
        .compare_json
        .as_deref()
        .map(|json| {
            serde_json::from_str::<ConformanceCompare>(json)
                .context("failed to parse MCP conformance compare JSON")
        })
        .transpose()?;
    let commits = compare
        .as_ref()
        .map(|compare| compare.commits.clone())
        .unwrap_or_default();
    let files = compare.map(|compare| compare.files).unwrap_or_default();
    let changed_terms = if drift {
        changed_terms_from_conformance_files(&files)
    } else {
        BTreeSet::new()
    };
    Ok(ConformanceReport {
        drift,
        baseline_sha,
        head_sha: head.sha,
        url: input.url.clone(),
        head_date: head.commit.author.date,
        head_message: head.commit.message,
        head_html_url: head.html_url,
        commits,
        files,
        impacts: if drift {
            scan_repo_impacts(&input.repo_root, &changed_terms)?
        } else {
            Vec::new()
        },
    })
}

fn append_schema_section(body: &mut String, report: &SchemaReport) {
    body.push_str("## MCP Schema Watch\n\n");
    body.push_str(&format!(
        "- Upstream schema: [{}]({})\n",
        report.url, report.url
    ));
    body.push_str(&format!("- Baseline SHA-256: `{}`\n", report.baseline_hash));
    body.push_str(&format!("- Upstream SHA-256: `{}`\n", report.upstream_hash));
    body.push_str(&format!("- Drift: `{}`\n\n", report.drift));
    if !report.commits.is_empty() {
        body.push_str("### Recent schema commits\n\n");
        for commit in report.commits.iter().take(5) {
            let summary = commit.commit.message.lines().next().unwrap_or("").trim();
            body.push_str(&format!(
                "- [`{}`]({}) `{}` {}\n",
                short_sha(&commit.sha),
                commit.html_url,
                commit.commit.author.date,
                summary
            ));
        }
        body.push('\n');
    }
    append_impact_section(
        body,
        "Potential schema impact in this repo",
        &report.impacts,
    );
    if report.drift {
        body.push_str("<details><summary>MCP schema diff</summary>\n\n");
        body.push_str("```diff\n");
        body.push_str(&report.diff);
        if !report.diff.ends_with('\n') {
            body.push('\n');
        }
        body.push_str("```\n\n</details>\n\n");
    }
}

fn append_conformance_section(body: &mut String, report: &ConformanceReport) {
    body.push_str("## MCP Conformance Watch\n\n");
    body.push_str(&format!(
        "- Upstream repo: [{}]({})\n",
        report.url, report.url
    ));
    body.push_str(&format!("- Baseline SHA: `{}`\n", report.baseline_sha));
    body.push_str(&format!(
        "- Head SHA: [`{}`]({})\n",
        short_sha(&report.head_sha),
        report.head_html_url
    ));
    body.push_str(&format!("- Head date: `{}`\n", report.head_date));
    body.push_str(&format!("- Drift: `{}`\n\n", report.drift));
    let head_summary = report.head_message.lines().next().unwrap_or("").trim();
    if !head_summary.is_empty() {
        body.push_str(&format!("Latest commit: {head_summary}\n\n"));
    }
    if !report.commits.is_empty() {
        body.push_str("### New conformance commits\n\n");
        for commit in report.commits.iter().take(10) {
            let summary = commit.commit.message.lines().next().unwrap_or("").trim();
            body.push_str(&format!(
                "- [`{}`]({}) `{}` {}\n",
                short_sha(&commit.sha),
                commit.html_url,
                commit.commit.author.date,
                summary
            ));
        }
        body.push('\n');
    }
    if !report.files.is_empty() {
        body.push_str("### Changed conformance files\n\n");
        body.push_str("| File | Status | +/- | Changes |\n");
        body.push_str("|---|---:|---:|---:|\n");
        for file in report.files.iter().take(20) {
            let file_link = file
                .blob_url
                .as_ref()
                .map(|url| format!("[`{}`]({url})", file.filename))
                .unwrap_or_else(|| format!("`{}`", file.filename));
            body.push_str(&format!(
                "| {file_link} | `{}` | +{} / -{} | {} |\n",
                file.status, file.additions, file.deletions, file.changes
            ));
        }
        if report.files.len() > 20 {
            body.push_str(&format!(
                "| _{} more files_ |  |  |  |\n",
                report.files.len() - 20
            ));
        }
        body.push('\n');
    }
    append_impact_section(
        body,
        "Potential conformance impact in this repo",
        &report.impacts,
    );
}

fn append_impact_section(body: &mut String, title: &str, impacts: &[RepoImpact]) {
    body.push_str(&format!("### {title}\n\n"));
    body.push_str("_Static identifier matches from upstream changes. Treat this as an inspection shortlist, not a complete migration plan._\n\n");
    if impacts.is_empty() {
        body.push_str("No direct local references to changed upstream terms were found.\n\n");
        return;
    }
    body.push_str("| Local file | Changed upstream terms referenced |\n");
    body.push_str("|---|---|\n");
    for impact in impacts.iter().take(25) {
        let terms = impact
            .identifiers
            .iter()
            .take(8)
            .map(|term| format!("`{term}`"))
            .collect::<Vec<_>>()
            .join(", ");
        body.push_str(&format!("| `{}` | {} |\n", impact.path, terms));
    }
    if impacts.len() > 25 {
        body.push_str(&format!("| _{} more files_ |  |\n", impacts.len() - 25));
    }
    body.push('\n');
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

fn simple_unified_diff(
    old_label: &str,
    new_label: &str,
    old: &str,
    new: &str,
    max_bytes: usize,
) -> String {
    let mut diff = String::new();
    diff.push_str(&format!("--- {old_label}\n"));
    diff.push_str(&format!("+++ {new_label}\n"));
    let old_lines = old.lines().collect::<Vec<_>>();
    let new_lines = new.lines().collect::<Vec<_>>();
    let max_len = old_lines.len().max(new_lines.len());
    for index in 0..max_len {
        match (old_lines.get(index), new_lines.get(index)) {
            (Some(left), Some(right)) if left == right => {}
            (Some(left), Some(right)) => {
                diff.push_str(&format!("@@ line {} @@\n", index + 1));
                diff.push_str(&format!("-{left}\n"));
                diff.push_str(&format!("+{right}\n"));
            }
            (Some(left), None) => {
                diff.push_str(&format!("@@ line {} @@\n", index + 1));
                diff.push_str(&format!("-{left}\n"));
            }
            (None, Some(right)) => {
                diff.push_str(&format!("@@ line {} @@\n", index + 1));
                diff.push_str(&format!("+{right}\n"));
            }
            (None, None) => {}
        }
        if diff.len() > max_bytes {
            diff.truncate(max_bytes);
            diff.push_str("\n... diff truncated ...\n");
            break;
        }
    }
    diff
}

fn changed_terms_from_text_diff(old: &str, new: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let old_lines = old.lines().collect::<Vec<_>>();
    let new_lines = new.lines().collect::<Vec<_>>();
    let max_len = old_lines.len().max(new_lines.len());
    for index in 0..max_len {
        match (old_lines.get(index), new_lines.get(index)) {
            (Some(left), Some(right)) if left == right => {}
            (Some(left), Some(right)) => {
                collect_identifiers(left, &mut terms);
                collect_identifiers(right, &mut terms);
            }
            (Some(left), None) => collect_identifiers(left, &mut terms),
            (None, Some(right)) => collect_identifiers(right, &mut terms),
            (None, None) => {}
        }
    }
    terms
}

fn changed_terms_from_conformance_files(files: &[ConformanceFile]) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    for file in files {
        collect_identifiers(&file.filename, &mut terms);
        if let Some(patch) = &file.patch {
            for line in patch.lines() {
                if line.starts_with('+') || line.starts_with('-') {
                    collect_identifiers(line, &mut terms);
                }
            }
        }
    }
    terms
}

fn collect_identifiers(text: &str, terms: &mut BTreeSet<String>) {
    let mut current = String::new();
    for ch in text.chars() {
        if ch == '_' || ch == '-' || ch.is_ascii_alphanumeric() {
            current.push(ch);
        } else {
            push_identifier(&current, terms);
            current.clear();
        }
    }
    push_identifier(&current, terms);
}

fn push_identifier(identifier: &str, terms: &mut BTreeSet<String>) {
    let trimmed = identifier.trim_matches(|ch: char| ch == '_' || ch == '-');
    if trimmed.len() < 3 || trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return;
    }
    let normalized = trimmed.replace('-', "_");
    if is_stop_identifier(&normalized) {
        return;
    }
    terms.insert(normalized);
}

fn is_stop_identifier(identifier: &str) -> bool {
    matches!(
        identifier,
        "add"
            | "all"
            | "and"
            | "any"
            | "api"
            | "are"
            | "arr"
            | "auth"
            | "body"
            | "bool"
            | "const"
            | "default"
            | "derive"
            | "else"
            | "enum"
            | "export"
            | "false"
            | "for"
            | "from"
            | "get"
            | "impl"
            | "interface"
            | "let"
            | "main"
            | "mod"
            | "new"
            | "not"
            | "null"
            | "number"
            | "object"
            | "one"
            | "option"
            | "pub"
            | "ref"
            | "self"
            | "serde"
            | "some"
            | "string"
            | "test"
            | "this"
            | "true"
            | "type"
            | "undefined"
            | "use"
            | "vec"
            | "with"
    )
}

fn scan_repo_impacts(root: &Path, terms: &BTreeSet<String>) -> Result<Vec<RepoImpact>> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut impacts = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_dir() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !is_repo_scan_file(path) || is_skipped_repo_path(&relative) {
            continue;
        }
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let matched = terms
            .iter()
            .filter(|term| text.contains(term.as_str()))
            .take(12)
            .cloned()
            .collect::<Vec<_>>();
        if matched.is_empty() {
            continue;
        }
        impacts.push(RepoImpact {
            path: relative,
            identifiers: matched,
        });
        if impacts.len() >= 40 {
            break;
        }
    }
    impacts.sort_by(|left, right| {
        right
            .identifiers
            .len()
            .cmp(&left.identifiers.len())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(impacts)
}

fn is_repo_scan_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "toml" | "json" | "yaml" | "yml" | "md" | "mdx" | "ts" | "tsx" | "js" | "jsx")
    )
}

fn is_skipped_repo_path(text: &str) -> bool {
    [
        ".git/",
        "target/",
        "node_modules/",
        "dist/",
        ".next/",
        "docs/references/mcp/schema/",
    ]
    .iter()
    .any(|needle| {
        text == needle.trim_end_matches('/')
            || text.starts_with(needle)
            || text.contains(&format!("/{needle}"))
    }) || text == "Cargo.lock"
        || text.ends_with("/Cargo.lock")
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
        let mut schema_baseline = None;
        let mut schema_upstream = None;
        let mut schema_commits_json = None;
        let mut schema_url =
            "https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/schema/2025-11-25/schema.ts"
                .to_owned();
        let mut conformance_baseline = None;
        let mut conformance_head_json = None;
        let mut conformance_compare_json = None;
        let mut conformance_url = "https://github.com/modelcontextprotocol/conformance".to_owned();
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
                "--schema-baseline" => {
                    index += 1;
                    schema_baseline =
                        Some(PathBuf::from(value_arg(args, index, "--schema-baseline")?));
                }
                "--schema-upstream" => {
                    index += 1;
                    schema_upstream =
                        Some(PathBuf::from(value_arg(args, index, "--schema-upstream")?));
                }
                "--schema-commits-json" => {
                    index += 1;
                    schema_commits_json = Some(PathBuf::from(value_arg(
                        args,
                        index,
                        "--schema-commits-json",
                    )?));
                }
                "--schema-url" => {
                    index += 1;
                    schema_url = value_arg(args, index, "--schema-url")?.to_owned();
                }
                "--conformance-baseline" => {
                    index += 1;
                    conformance_baseline = Some(PathBuf::from(value_arg(
                        args,
                        index,
                        "--conformance-baseline",
                    )?));
                }
                "--conformance-head-json" => {
                    index += 1;
                    conformance_head_json = Some(PathBuf::from(value_arg(
                        args,
                        index,
                        "--conformance-head-json",
                    )?));
                }
                "--conformance-compare-json" => {
                    index += 1;
                    conformance_compare_json = Some(PathBuf::from(value_arg(
                        args,
                        index,
                        "--conformance-compare-json",
                    )?));
                }
                "--conformance-url" => {
                    index += 1;
                    conformance_url = value_arg(args, index, "--conformance-url")?.to_owned();
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
                    "Usage: cargo xtask rmcp-release-monitor --crate-json rmcp.json --releases-json releases.json --issue-body issue.md [--schema-baseline schema.ts --schema-upstream upstream.ts] [--conformance-baseline main.sha --conformance-head-json head.json] [--current-version VERSION] [--max-body-bytes N]"
                ),
                unknown => bail!("unknown rmcp-release-monitor option: {unknown}"),
            }
            index += 1;
        }
        Ok(Self {
            crate_json: crate_json.context("--crate-json is required")?,
            releases_json: releases_json.context("--releases-json is required")?,
            issue_body: issue_body.context("--issue-body is required")?,
            schema_baseline,
            schema_upstream,
            schema_commits_json,
            schema_url,
            conformance_baseline,
            conformance_head_json,
            conformance_compare_json,
            conformance_url,
            current_version,
            max_body_bytes,
        })
    }

    fn schema_input(&self) -> Result<Option<SchemaMonitorInput>> {
        match (&self.schema_baseline, &self.schema_upstream) {
            (Some(baseline), Some(upstream)) => Ok(Some(SchemaMonitorInput {
                baseline: fs::read_to_string(baseline)
                    .with_context(|| format!("failed to read {}", baseline.display()))?,
                upstream: fs::read_to_string(upstream)
                    .with_context(|| format!("failed to read {}", upstream.display()))?,
                commits_json: self
                    .schema_commits_json
                    .as_ref()
                    .map(|path| {
                        fs::read_to_string(path)
                            .with_context(|| format!("failed to read {}", path.display()))
                    })
                    .transpose()?,
                url: self.schema_url.clone(),
                repo_root: PathBuf::from("."),
            })),
            (None, None) => Ok(None),
            _ => bail!("--schema-baseline and --schema-upstream must be provided together"),
        }
    }

    fn conformance_input(&self) -> Result<Option<ConformanceMonitorInput>> {
        match (&self.conformance_baseline, &self.conformance_head_json) {
            (Some(baseline), Some(head_json)) => Ok(Some(ConformanceMonitorInput {
                baseline_sha: fs::read_to_string(baseline)
                    .with_context(|| format!("failed to read {}", baseline.display()))?,
                head_json: fs::read_to_string(head_json)
                    .with_context(|| format!("failed to read {}", head_json.display()))?,
                compare_json: self
                    .conformance_compare_json
                    .as_ref()
                    .map(|path| {
                        fs::read_to_string(path)
                            .with_context(|| format!("failed to read {}", path.display()))
                    })
                    .transpose()?,
                url: self.conformance_url.clone(),
                repo_root: PathBuf::from("."),
            })),
            (None, None) => Ok(None),
            _ => bail!(
                "--conformance-baseline and --conformance-head-json must be provided together"
            ),
        }
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
        assert!(report
            .issue_body
            .contains("schema: allow null for Task.ttl"));
        assert!(report
            .issue_body
            .contains("Potential schema impact in this repo"));
        assert!(report
            .issue_body
            .contains("crates/soma/mcp/src/rmcp_server.rs"));
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
        assert!(report
            .issue_body
            .contains("authorization-server-metadata.ts"));
        assert!(report
            .issue_body
            .contains("Potential conformance impact in this repo"));
        assert!(report
            .issue_body
            .contains("crates/soma/runtime/src/server.rs"));
        assert!(report
            .issue_body
            .contains("`client_id_metadata_document_supported`"));
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
        let truncated = clamp_issue_body(body, 230);

        assert!(truncated.contains("rmcp-release-monitor-truncated"));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }
}
