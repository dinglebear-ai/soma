//! MCP conformance drift watch.
//!
//! Compares the pinned conformance-suite commit against upstream `main`, and -
//! when they diverge - renders the new commits, the changed suite files, and
//! the local impact shortlist into the issue body.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

use super::diagnostics::json_parse_context;
use super::impact::{append_impact_section, collect_identifiers, scan_repo_impacts, RepoImpact};
use super::short_sha;

#[derive(Debug)]
pub(super) struct ConformanceMonitorInput {
    pub(super) baseline_sha: String,
    pub(super) head_json: String,
    pub(super) compare_json: Option<String>,
    pub(super) url: String,
    pub(super) repo_root: PathBuf,
}

#[derive(Debug)]
pub(super) struct ConformanceReport {
    pub(super) drift: bool,
    pub(super) baseline_sha: String,
    pub(super) head_sha: String,
    url: String,
    head_date: String,
    head_message: String,
    head_html_url: String,
    commits: Vec<ConformanceCommit>,
    files: Vec<ConformanceFile>,
    impacts: Vec<RepoImpact>,
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

pub(super) fn build_conformance_report(
    input: &ConformanceMonitorInput,
) -> Result<ConformanceReport> {
    let head: ConformanceHead = serde_json::from_str(&input.head_json).with_context(|| {
        json_parse_context(
            "MCP conformance head JSON (--conformance-head-json)",
            &input.head_json,
        )
    })?;
    let baseline_sha = input.baseline_sha.trim().to_owned();
    let drift = baseline_sha != head.sha;
    let compare = input
        .compare_json
        .as_deref()
        .map(|json| {
            serde_json::from_str::<ConformanceCompare>(json).with_context(|| {
                json_parse_context(
                    "MCP conformance compare JSON (--conformance-compare-json)",
                    json,
                )
            })
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

pub(super) fn append_conformance_section(body: &mut String, report: &ConformanceReport) {
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
