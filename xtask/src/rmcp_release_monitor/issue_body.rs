//! Markdown assembly for the monitor's GitHub issue body.
//!
//! Takes the already-built reports and turns them into the single string the
//! workflow writes to disk. Nothing here decides *whether* there is drift; it
//! only renders what the report builders found, then clamps the result to
//! GitHub's issue-body size limit.

use anyhow::Result;
use semver::Version;

use super::conformance::{append_conformance_section, ConformanceReport};
use super::schema::{append_schema_section, SchemaReport};
use super::{CrateVersion, CratesIoResponse, GithubRelease, MARKER};

pub(super) fn render_issue_body(
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

pub(super) fn clamp_issue_body(mut body: String, max_body_bytes: usize) -> String {
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
