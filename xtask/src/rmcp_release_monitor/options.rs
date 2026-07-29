//! CLI surface for `cargo xtask rmcp-release-monitor`.
//!
//! Parses the flag list into `Options`, then reads the referenced files into
//! the two watch inputs. Keeping the flag table here means adding a monitor
//! input touches one module instead of the orchestrator.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;

use super::DEFAULT_MAX_BODY_BYTES;
use super::conformance::ConformanceMonitorInput;
use super::schema::SchemaMonitorInput;

#[derive(Debug)]
pub(super) struct Options {
    pub(super) crate_json: PathBuf,
    pub(super) releases_json: PathBuf,
    pub(super) issue_body: PathBuf,
    schema_baseline: Option<PathBuf>,
    schema_upstream: Option<PathBuf>,
    schema_commits_json: Option<PathBuf>,
    schema_url: String,
    conformance_baseline: Option<PathBuf>,
    conformance_head_json: Option<PathBuf>,
    conformance_compare_json: Option<PathBuf>,
    conformance_url: String,
    pub(super) current_version: Option<String>,
    pub(super) max_body_bytes: usize,
}

impl Options {
    pub(super) fn parse(args: &[String]) -> Result<Self> {
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

    pub(super) fn schema_input(&self) -> Result<Option<SchemaMonitorInput>> {
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

    pub(super) fn conformance_input(&self) -> Result<Option<ConformanceMonitorInput>> {
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
