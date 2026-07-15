//! `cargo xtask codex-schema regen <dir>` - rebuilds the vendored schema
//! assets from a fresh `codex app-server generate-json-schema` dump, and
//! stamps the `codex` version they were generated from for staleness
//! detection (see `crates/codex-app-server-client/build.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use super::{
    load_combined_defs, merge, parse_gen_dir, CODEX_VERSION_PATH, METHODS_JSON_PATH,
    PROTOCOL_SCHEMA_PATH,
};

pub fn run(args: &[String]) -> Result<()> {
    let gen_dir = parse_args(args)?;
    regen(&gen_dir)
}

fn parse_args(args: &[String]) -> Result<PathBuf> {
    parse_gen_dir(
        args,
        "Usage: cargo xtask codex-schema regen <path-to-codex-generate-json-schema-output-dir>\n\
         Generate that directory first with:\n  \
         codex app-server generate-json-schema --out <dir> --experimental",
    )
}

pub fn regen(gen_dir: &Path) -> Result<()> {
    // Captured (and its file written) first, before either schema file is
    // touched: `codex --version` failing (e.g. not on PATH) is the one step
    // in this function most likely to fail on a maintainer's machine, and
    // doing it last used to leave the working tree in a confusing
    // partially-regenerated state (both schema files rewritten, but the
    // version stamp untouched) if it failed. Front-loading it means a
    // failure here leaves nothing changed at all.
    let version = capture_codex_version()?;

    let (combined, combined_defs) = load_combined_defs(gen_dir)?;

    let protocol_text =
        serde_json::to_string_pretty(&combined).context("serialize combined schema")?;
    assert_no_v2_refs(&protocol_text)?;

    let manifest = merge::build_methods_manifest(&combined_defs)?;
    // Serializes the struct directly to a string rather than going through an
    // intermediate `serde_json::Value` first: `RequestEntry`/`NotificationEntry`
    // derive `Serialize`, so a struct's field order is always its declaration
    // order regardless of `serde_json::Map`'s own key-ordering behavior - but
    // routing through `to_value()` first would lose that guarantee, since the
    // resulting `Value::Object`'s `Map` re-sorts keys unless the crate-wide
    // (and workspace-unifying) `preserve_order` feature is enabled.
    let methods_text =
        serde_json::to_string_pretty(&manifest).context("serialize methods manifest")?;

    fs::write(PROTOCOL_SCHEMA_PATH, &protocol_text)
        .with_context(|| format!("write {PROTOCOL_SCHEMA_PATH}"))?;
    fs::write(METHODS_JSON_PATH, &methods_text)
        .with_context(|| format!("write {METHODS_JSON_PATH}"))?;

    eprintln!("total definitions: {}", combined_defs.len());
    eprintln!("wrote {PROTOCOL_SCHEMA_PATH}");

    let missing_response: Vec<&str> = manifest
        .client_requests
        .iter()
        .chain(&manifest.server_requests)
        .filter(|e| e.response_type.is_none())
        .map(|e| e.method.as_str())
        .collect();
    eprintln!(
        "client_requests={} server_requests={} server_notifications={} client_notifications={}",
        manifest.client_requests.len(),
        manifest.server_requests.len(),
        manifest.server_notifications.len(),
        manifest.client_notifications.len(),
    );
    eprintln!("methods with no resolvable response type: {missing_response:?}");
    eprintln!("wrote {METHODS_JSON_PATH}");

    stamp_codex_version(&version)?;

    Ok(())
}

/// Mirrors the Python original's final sanity assertion: every
/// `#/definitions/v2/X` ref must have been rewritten to `#/definitions/X`
/// before writing - a leftover match means the ref-rewrite pass missed
/// something and the output would fail to resolve.
fn assert_no_v2_refs(serialized_protocol_schema: &str) -> Result<()> {
    let count = serialized_protocol_schema
        .matches("#/definitions/v2/")
        .count();
    eprintln!("remaining v2-prefixed refs (must be 0): {count}");
    if count != 0 {
        bail!("ref rewrite incomplete: found {count} remaining \"#/definitions/v2/\" ref(s)");
    }
    Ok(())
}

fn stamp_codex_version(version: &str) -> Result<()> {
    fs::write(CODEX_VERSION_PATH, format!("{version}\n"))
        .with_context(|| format!("write {CODEX_VERSION_PATH}"))?;
    eprintln!("stamped {CODEX_VERSION_PATH}: {version:?}");
    Ok(())
}

fn capture_codex_version() -> Result<String> {
    let output = Command::new("codex")
        .arg("--version")
        .output()
        .context("failed to run `codex --version` - is the codex CLI installed and on PATH?")?;
    if !output.status.success() {
        bail!(
            "`codex --version` exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let raw =
        String::from_utf8(output.stdout).context("`codex --version` emitted non-UTF-8 output")?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("`codex --version` produced no output");
    }
    Ok(trimmed.to_string())
}
