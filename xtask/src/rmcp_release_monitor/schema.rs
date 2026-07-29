//! MCP schema drift watch.
//!
//! Hashes the pinned schema mirror against the upstream copy, and - when they
//! diverge - renders the diff, the recent upstream commits, and the local
//! impact shortlist into the issue body.

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;

use super::diagnostics::json_parse_context;
use super::impact::{RepoImpact, append_impact_section, collect_identifiers, scan_repo_impacts};
use super::short_sha;

#[derive(Debug)]
pub(super) struct SchemaMonitorInput {
    pub(super) baseline: String,
    pub(super) upstream: String,
    pub(super) commits_json: Option<String>,
    pub(super) url: String,
    pub(super) repo_root: PathBuf,
}

#[derive(Debug)]
pub(super) struct SchemaReport {
    pub(super) drift: bool,
    pub(super) baseline_hash: String,
    pub(super) upstream_hash: String,
    url: String,
    diff: String,
    commits: Vec<SchemaCommit>,
    impacts: Vec<RepoImpact>,
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

pub(super) fn build_schema_report(input: &SchemaMonitorInput) -> Result<SchemaReport> {
    let baseline_hash = sha256_hex(input.baseline.as_bytes());
    let upstream_hash = sha256_hex(input.upstream.as_bytes());
    let drift = baseline_hash != upstream_hash;
    let commits = input
        .commits_json
        .as_deref()
        .map(|json| {
            serde_json::from_str(json).with_context(|| {
                json_parse_context("MCP schema commit JSON (--schema-commits-json)", json)
            })
        })
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

pub(super) fn append_schema_section(body: &mut String, report: &SchemaReport) {
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

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
