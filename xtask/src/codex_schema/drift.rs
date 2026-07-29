//! `cargo xtask codex-schema drift [--dir <dump-dir>] [--json] [--strict]` -
//! diffs the vendored `schema/methods.json` + `schema/CODEX_VERSION.txt`
//! against a fresh `codex app-server generate-json-schema` dump (or an
//! existing dump directory passed via `--dir`), so an upstream `codex`
//! version bump that adds, removes, or renames a method can't silently slip
//! past the version-string-only staleness check in
//! `crates/shared/codex-app-server-client/build.rs`.
//!
//! Reuses `regen`'s exact schema-load + manifest-build code path
//! (`load_combined_defs` + `merge::build_methods_manifest`) to build the
//! "installed" manifest in memory - this module never reimplements schema
//! parsing or method extraction, it only diffs two already-built manifests.
//! See `crates/shared/codex-app-server-client/README.md`'s "Regenerating the
//! schema" section, `build.rs`'s non-fatal staleness warning, and
//! `codex_app_server_client::compat::CompatibilityReport` (schema-version +
//! method-count only) for the prior art this check sharpens into a real diff.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;

use super::{CODEX_VERSION_PATH, METHODS_JSON_PATH, load_combined_defs, merge};

const USAGE: &str = "Usage: cargo xtask codex-schema drift [--dir <path-to-codex-generate-json-schema-output-dir>] [--json] [--strict]

Diffs the vendored schema/methods.json + schema/CODEX_VERSION.txt against a
fresh `codex app-server generate-json-schema --out <dir> --experimental` dump
(or an existing dump directory passed via --dir) and reports added, removed,
and changed methods per section (client_requests, server_requests,
server_notifications, client_notifications).

  --dir <dir>  Diff against an already-generated dump directory instead of
               shelling out to `codex` to produce a fresh one. Makes the
               check testable/reusable in CI without requiring `codex` on
               PATH for the dump step itself.
  --json       Emit a machine-readable report instead of the human-readable
               one.
  --strict     Exit non-zero when drift is found. Without --strict, drift is
               still reported loudly but the command exits 0.

Missing `codex` on PATH (when --dir is not given) is never a hard failure:
the check prints \"skipped\" and exits 0, mirroring build.rs's staleness
check and tests/smoke.rs's live-integration test.";

/// Section names in the vendored/installed `methods.json` shape, in the
/// fixed order the report is always rendered in.
const SECTIONS: &[&str] = &[
    "client_requests",
    "server_requests",
    "server_notifications",
    "client_notifications",
];

pub fn run(args: &[String]) -> Result<()> {
    let options = parse_args(args)?;
    match compute_outcome(&options)? {
        Outcome::Skipped(reason) => {
            if options.json {
                println!("{}", serde_json::json!({"skipped": true, "reason": reason}));
            } else {
                println!("codex-schema drift: skipped: {reason}");
            }
            Ok(())
        }
        Outcome::Report(report) => {
            if options.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("serialize drift report as JSON")?
                );
            } else {
                print_human_report(&report);
            }
            if !report.in_sync && options.strict {
                bail!(
                    "codex-schema drift: {} method(s) drifted from the vendored schema (--strict) \
                     - review the report above, then run `cargo xtask codex-schema regen <dir>` \
                     once the change is intentional.",
                    report.drifted_count
                );
            }
            Ok(())
        }
    }
}

#[derive(Debug)]
struct DriftOptions {
    dir: Option<PathBuf>,
    json: bool,
    strict: bool,
}

fn parse_args(args: &[String]) -> Result<DriftOptions> {
    let mut dir = None;
    let mut json = false;
    let mut strict = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .with_context(|| format!("--dir requires a value\n\n{USAGE}"))?;
                dir = Some(PathBuf::from(value));
            }
            "--json" => json = true,
            "--strict" => strict = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unexpected argument: {other}\n\n{USAGE}"),
        }
        index += 1;
    }
    Ok(DriftOptions { dir, json, strict })
}

enum Outcome {
    Skipped(String),
    Report(DriftReport),
}

/// Builds the diff outcome: either a skip reason (no `codex` on PATH and no
/// `--dir`) or a full `DriftReport`. Keeps the temp dump directory (when one
/// is created) alive for the duration of the manifest build via the returned
/// `TempDir` guard's drop timing.
fn compute_outcome(options: &DriftOptions) -> Result<Outcome> {
    let vendored_version = load_vendored_version()?;
    let vendored_manifest = load_vendored_manifest()?;

    let (gen_dir, installed_version, _dump_guard) = match &options.dir {
        Some(dir) => {
            if !dir.exists() {
                bail!(
                    "--dir {} does not exist - generate it first with:\n  \
                     codex app-server generate-json-schema --out {} --experimental",
                    dir.display(),
                    dir.display()
                );
            }
            (dir.clone(), installed_version_best_effort(), None)
        }
        None => {
            let Some(installed_version) = codex_version_or_skip()? else {
                return Ok(Outcome::Skipped(
                    "no codex on PATH - cannot generate a fresh schema dump to diff against. \
                     Install the codex CLI, or pass --dir <existing-dump> to diff a saved \
                     `codex app-server generate-json-schema` output."
                        .to_string(),
                ));
            };
            let dump =
                tempfile::tempdir().context("create temp dir for a fresh codex schema dump")?;
            generate_fresh_dump(dump.path())?;
            let gen_dir = dump.path().to_path_buf();
            (gen_dir, Some(installed_version), Some(dump))
        }
    };

    let installed_manifest = build_installed_manifest(&gen_dir)?;
    let report = build_report(
        &vendored_manifest,
        &installed_manifest,
        &vendored_version,
        installed_version.as_deref(),
    )?;
    Ok(Outcome::Report(report))
}

/// Runs `codex --version` and distinguishes "codex is not on PATH at all"
/// (the one condition this check must never hard-fail on) from every other
/// failure mode (codex present but broken), which are real errors.
fn codex_version_or_skip() -> Result<Option<String>> {
    match Command::new("codex").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let raw = String::from_utf8(output.stdout)
                .context("`codex --version` emitted non-UTF-8 output")?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                bail!("`codex --version` produced no output");
            }
            Ok(Some(trimmed.to_string()))
        }
        Ok(output) => bail!(
            "`codex --version` exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).context("failed to run `codex --version`"),
    }
}

/// Best-effort installed-version lookup for `--dir` mode: `codex` is purely
/// informational there (the dump already exists), so any failure is silently
/// treated as "unknown" rather than blocking the diff - mirrors
/// `build.rs`'s `check_codex_staleness`.
fn installed_version_best_effort() -> Option<String> {
    let output = Command::new("codex").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Shells out to the exact command documented in this crate's `regen`
/// workflow (`codex_schema.rs`'s `print_help`) to produce a fresh dump.
fn generate_fresh_dump(out_dir: &Path) -> Result<()> {
    let status = Command::new("codex")
        .arg("app-server")
        .arg("generate-json-schema")
        .arg("--out")
        .arg(out_dir)
        .arg("--experimental")
        .status()
        .context("failed to run `codex app-server generate-json-schema`")?;
    if !status.success() {
        bail!(
            "`codex app-server generate-json-schema --out {} --experimental` exited with status {status}",
            out_dir.display()
        );
    }
    Ok(())
}

fn load_vendored_manifest() -> Result<Value> {
    super::read_json(Path::new(METHODS_JSON_PATH))
}

fn load_vendored_version() -> Result<String> {
    let text = fs::read_to_string(CODEX_VERSION_PATH)
        .with_context(|| format!("read {CODEX_VERSION_PATH}"))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("{CODEX_VERSION_PATH} is empty");
    }
    Ok(trimmed.to_string())
}

/// Builds the "installed" manifest via the exact same code path `regen` uses
/// to write `methods.json` (`load_combined_defs` + `build_methods_manifest`),
/// then converts it to a `Value` so it can be diffed with the same
/// section-shaped logic as the vendored JSON file - no separate parsing path
/// to drift out of sync with the real one.
fn build_installed_manifest(gen_dir: &Path) -> Result<Value> {
    let (_, combined_defs) = load_combined_defs(gen_dir)?;
    let manifest = merge::build_methods_manifest(&combined_defs)?;
    serde_json::to_value(&manifest)
        .context("serialize installed methods manifest to JSON for diffing")
}

/// One method's diffable signature: everything about it other than its name
/// that `regen` derives from the schema. Notification entries never carry a
/// `response_type` key in the manifest JSON, so `response_type` is `None` for
/// every notification-section entry - correctly comparing "no key" in both
/// vendored and installed as equal, rather than a spurious "changed".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MethodSignature {
    params_type: Option<String>,
    params_optional: bool,
    response_type: Option<String>,
}

/// Parses one manifest's section array (`client_requests`, etc.) into a
/// `method -> signature` map. Shared by both the vendored JSON file and the
/// installed manifest (also serialized to `Value`), so both sides go through
/// identical extraction logic - a field name typo here would otherwise show
/// up as one specific, hard-to-diagnose false "changed" or "added"/"removed"
/// pair instead of a loud, obvious error.
fn section_map(manifest: &Value, section: &str) -> Result<BTreeMap<String, MethodSignature>> {
    let entries = manifest
        .get(section)
        .and_then(Value::as_array)
        .with_context(|| format!("manifest missing \"{section}\" array"))?;

    let mut map = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let method = entry
            .get("method")
            .and_then(Value::as_str)
            .with_context(|| format!("{section}[{index}] is missing a string \"method\" field"))?
            .to_string();
        let signature = MethodSignature {
            params_type: entry
                .get("params_type")
                .and_then(Value::as_str)
                .map(str::to_string),
            params_optional: entry
                .get("params_optional")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            response_type: entry
                .get("response_type")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        if map.insert(method.clone(), signature).is_some() {
            bail!("{section}: duplicate method {method:?} in manifest - malformed input");
        }
    }
    Ok(map)
}

#[derive(Debug, Clone, Serialize)]
struct MethodEntry {
    method: String,
    params_type: Option<String>,
    params_optional: bool,
    response_type: Option<String>,
}

impl MethodEntry {
    fn new(method: &str, signature: &MethodSignature) -> Self {
        Self {
            method: method.to_string(),
            params_type: signature.params_type.clone(),
            params_optional: signature.params_optional,
            response_type: signature.response_type.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ChangedMethod {
    method: String,
    vendored: MethodSignature,
    installed: MethodSignature,
}

#[derive(Debug, Clone, Serialize)]
struct SectionDiff {
    section: String,
    vendored_count: usize,
    installed_count: usize,
    added: Vec<MethodEntry>,
    removed: Vec<MethodEntry>,
    changed: Vec<ChangedMethod>,
}

impl SectionDiff {
    fn drifted_count(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }

    fn in_sync(&self) -> bool {
        self.drifted_count() == 0
    }
}

/// Diffs one section (`client_requests`, `server_requests`,
/// `server_notifications`, or `client_notifications`) between the vendored
/// and installed manifests. `added`/`removed`/`changed` are each sorted by
/// method name for stable, scannable output regardless of the underlying
/// `BTreeMap` iteration order (which is already method-name order, but the
/// explicit sort documents the intent and survives a future change to how
/// the maps are built).
fn diff_section(section: &str, vendored: &Value, installed: &Value) -> Result<SectionDiff> {
    let vendored_map = section_map(vendored, section)?;
    let installed_map = section_map(installed, section)?;

    let mut added = Vec::new();
    let mut changed = Vec::new();
    for (method, installed_sig) in &installed_map {
        match vendored_map.get(method) {
            None => added.push(MethodEntry::new(method, installed_sig)),
            Some(vendored_sig) if vendored_sig != installed_sig => changed.push(ChangedMethod {
                method: method.clone(),
                vendored: vendored_sig.clone(),
                installed: installed_sig.clone(),
            }),
            Some(_) => {}
        }
    }

    let mut removed = Vec::new();
    for (method, vendored_sig) in &vendored_map {
        if !installed_map.contains_key(method) {
            removed.push(MethodEntry::new(method, vendored_sig));
        }
    }

    added.sort_by(|a, b| a.method.cmp(&b.method));
    removed.sort_by(|a, b| a.method.cmp(&b.method));
    changed.sort_by(|a, b| a.method.cmp(&b.method));

    Ok(SectionDiff {
        section: section.to_string(),
        vendored_count: vendored_map.len(),
        installed_count: installed_map.len(),
        added,
        removed,
        changed,
    })
}

#[derive(Debug, Clone, Serialize)]
struct VersionDelta {
    vendored: String,
    installed: Option<String>,
    /// `None` when the installed version could not be determined (e.g.
    /// `--dir` was used and `codex` isn't on PATH either); `Some(false)`
    /// when it's known and differs from the vendored stamp.
    matches: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct DriftReport {
    /// True only when every section has zero added/removed/changed methods.
    /// A version-string mismatch with an otherwise identical method surface
    /// does NOT by itself count as drift here - `build.rs`'s staleness
    /// warning already covers that case; this report's job is to make method
    /// *surface* changes impossible to miss.
    in_sync: bool,
    drifted_count: usize,
    version: VersionDelta,
    sections: Vec<SectionDiff>,
}

fn build_report(
    vendored_manifest: &Value,
    installed_manifest: &Value,
    vendored_version: &str,
    installed_version: Option<&str>,
) -> Result<DriftReport> {
    let sections = SECTIONS
        .iter()
        .map(|section| diff_section(section, vendored_manifest, installed_manifest))
        .collect::<Result<Vec<_>>>()?;
    let drifted_count = sections.iter().map(SectionDiff::drifted_count).sum();
    let version = VersionDelta {
        vendored: vendored_version.to_string(),
        installed: installed_version.map(str::to_string),
        matches: installed_version.map(|installed| installed == vendored_version),
    };
    Ok(DriftReport {
        in_sync: drifted_count == 0,
        drifted_count,
        version,
        sections,
    })
}

fn print_human_report(report: &DriftReport) {
    if report.in_sync {
        println!("codex-schema drift: in sync");
    } else {
        println!(
            "codex-schema drift: {} method(s) drifted from the vendored schema",
            report.drifted_count
        );
    }

    match (&report.version.installed, report.version.matches) {
        (Some(installed), Some(true)) => {
            println!("  codex version: {installed} (matches vendored)");
        }
        (Some(installed), Some(false)) => {
            println!(
                "  codex version: vendored={} installed={} (MISMATCH)",
                report.version.vendored, installed
            );
        }
        _ => {
            println!(
                "  codex version: vendored={} installed=unknown (codex --version unavailable)",
                report.version.vendored
            );
        }
    }
    println!();

    for section in &report.sections {
        if section.in_sync() {
            println!(
                "{}: {} -> {} (in sync)",
                section.section, section.vendored_count, section.installed_count
            );
            continue;
        }
        println!(
            "{}: {} -> {} (+{} added, -{} removed, ~{} changed)",
            section.section,
            section.vendored_count,
            section.installed_count,
            section.added.len(),
            section.removed.len(),
            section.changed.len()
        );
        for entry in &section.added {
            println!(
                "    + {} (params={:?} optional={} response={:?})",
                entry.method, entry.params_type, entry.params_optional, entry.response_type
            );
        }
        for entry in &section.removed {
            println!(
                "    - {} (params={:?} optional={} response={:?})",
                entry.method, entry.params_type, entry.params_optional, entry.response_type
            );
        }
        for changed in &section.changed {
            println!("    ~ {}", changed.method);
            if changed.vendored.params_type != changed.installed.params_type {
                println!(
                    "        params_type: {:?} -> {:?}",
                    changed.vendored.params_type, changed.installed.params_type
                );
            }
            if changed.vendored.params_optional != changed.installed.params_optional {
                println!(
                    "        params_optional: {} -> {}",
                    changed.vendored.params_optional, changed.installed.params_optional
                );
            }
            if changed.vendored.response_type != changed.installed.response_type {
                println!(
                    "        response_type: {:?} -> {:?}",
                    changed.vendored.response_type, changed.installed.response_type
                );
            }
        }
    }

    println!();
    if report.in_sync {
        println!("no remediation needed.");
    } else {
        println!(
            "remediation: cargo xtask codex-schema regen <path-to-codex-generate-json-schema-output-dir>"
        );
    }
}

#[cfg(test)]
#[path = "drift_tests.rs"]
mod tests;
