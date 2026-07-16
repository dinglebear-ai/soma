use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::release_versions;

pub(crate) fn check(root: &Path, args: &[String]) -> Result<()> {
    let options = ReleaseCommandOptions::parse(args)?;
    release_versions::check(
        root,
        options.base.as_deref(),
        &options.head,
        options.mode,
        options.json,
    )
}

pub(crate) fn plan(root: &Path, args: &[String]) -> Result<()> {
    let options = ReleaseCommandOptions::parse(args)?;
    let plans = release_versions::plan(root, options.base.as_deref(), &options.head, options.mode)?;
    release_versions::print_plans(&plans, options.json)
}

pub(crate) fn bump(root: &Path, args: &[String]) -> Result<()> {
    if args.len() != 2 {
        bail!("Usage: cargo xtask bump-version <component> <patch|minor|major>");
    }
    let level = parse_bump_level(&args[1])?;
    release_versions::bump(root, &args[0], level)
}

#[derive(Debug, PartialEq, Eq)]
struct ReleaseCommandOptions {
    base: Option<String>,
    head: String,
    mode: release_versions::GateMode,
    json: bool,
}

impl ReleaseCommandOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut base = None;
        let mut head = "HEAD".to_owned();
        let mut mode = release_versions::GateMode::Pr;
        let mut json = false;
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--base" => {
                    index += 1;
                    base = Some(
                        args.get(index)
                            .context("--base requires a value")?
                            .to_owned(),
                    );
                }
                "--head" => {
                    index += 1;
                    head = args
                        .get(index)
                        .context("--head requires a value")?
                        .to_owned();
                }
                "--mode" => {
                    index += 1;
                    mode = parse_gate_mode(args.get(index).context("--mode requires a value")?)?;
                }
                "--json" => json = true,
                "--help" | "-h" => {
                    bail!("Usage: cargo xtask <check-release-versions|release-plan> [--base REF] [--head REF] [--mode pr|main] [--json]");
                }
                unknown => bail!("unknown release option: {unknown}"),
            }
            index += 1;
        }
        Ok(Self {
            base,
            head,
            mode,
            json,
        })
    }
}

fn parse_gate_mode(value: &str) -> Result<release_versions::GateMode> {
    match value {
        "pr" => Ok(release_versions::GateMode::Pr),
        "main" => Ok(release_versions::GateMode::Main),
        other => bail!("unknown release gate mode {other:?}; expected pr or main"),
    }
}

fn parse_bump_level(value: &str) -> Result<release_versions::BumpLevel> {
    match value {
        "patch" => Ok(release_versions::BumpLevel::Patch),
        "minor" => Ok(release_versions::BumpLevel::Minor),
        "major" => Ok(release_versions::BumpLevel::Major),
        other => bail!("unknown bump level {other:?}; expected patch, minor, or major"),
    }
}

#[cfg(test)]
#[path = "release_commands_tests.rs"]
mod tests;
