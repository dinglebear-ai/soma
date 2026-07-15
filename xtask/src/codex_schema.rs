//! `cargo xtask codex-schema <subcommand>` — Rust port of the vendored
//! `codex-app-server-client` crate's former `schema/build_combined_schema.py`,
//! plus staleness stamping and typify-panic bisection tooling.
//!
//! Subcommands:
//!   regen   Rebuild schema/protocol.schema.json + schema/methods.json from a
//!           fresh `codex app-server generate-json-schema` dump, and stamp the
//!           `codex` version the vendored schema was generated from.
//!   bisect  Binary-search a fresh schema dump for the minimal definition(s)
//!           that panic typify's schema-merge logic, the same failure mode
//!           documented for `McpServerElicitationRequestParams` in the
//!           crate's README.
//!
//! See `crates/codex-app-server-client/README.md`'s "Regenerating the
//! schema" section for the end-to-end workflow this drives.

mod bisect;
mod merge;
mod naming;
mod regen;
mod typify_probe;

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// Workspace-relative path to the vendored combined protocol schema.
pub(crate) const PROTOCOL_SCHEMA_PATH: &str =
    "crates/codex-app-server-client/schema/protocol.schema.json";
/// Workspace-relative path to the vendored per-method manifest.
pub(crate) const METHODS_JSON_PATH: &str = "crates/codex-app-server-client/schema/methods.json";
/// Workspace-relative path to the staleness-tracking codex version stamp
/// (see `regen::stamp_codex_version` and `build.rs`'s staleness check).
pub(crate) const CODEX_VERSION_PATH: &str =
    "crates/codex-app-server-client/schema/CODEX_VERSION.txt";

/// Filename of the master (non-v2) schema bundle emitted by
/// `codex app-server generate-json-schema` into its output directory.
/// Shared by `regen` and `bisect`, which both read the same dump directory -
/// must stay byte-identical between the two or one could silently start
/// reading a stale/mismatched file.
pub(crate) const MASTER_BUNDLE_FILE: &str = "codex_app_server_protocol.schemas.json";
/// Filename of the v2-only schema bundle emitted alongside the master
/// bundle. See [`MASTER_BUNDLE_FILE`].
pub(crate) const V2_BUNDLE_FILE: &str = "codex_app_server_protocol.v2.schemas.json";

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("regen") => regen::run(&args[1..]),
        Some("bisect") => bisect::run(&args[1..]),
        Some("--help") | Some("-h") | Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(unknown) => bail!(
            "Unknown codex-schema subcommand: {unknown:?}\nRun `cargo xtask codex-schema --help` for usage."
        ),
    }
}

fn print_help() {
    eprintln!(
        "cargo xtask codex-schema — codex-app-server-client schema tooling

USAGE:
  cargo xtask codex-schema <subcommand> <path-to-codex-generate-json-schema-output-dir>

SUBCOMMANDS:
  regen <dir>   Rebuild schema/protocol.schema.json + schema/methods.json from
                a `codex app-server generate-json-schema --out <dir> --experimental`
                dump, and stamp schema/CODEX_VERSION.txt with `codex --version`.
  bisect <dir>  Binary-search a fresh schema dump for the definition(s) that
                panic typify's schema-merge logic (see README.md).
  help          Show this help

Typical workflow after upgrading the installed `codex` CLI:
  codex app-server generate-json-schema --out /tmp/codex-schema --experimental
  cargo xtask codex-schema regen /tmp/codex-schema
  cargo build -p codex-app-server-client --all-targets
  cargo clippy -p codex-app-server-client --all-targets -- -D warnings
  cargo test -p codex-app-server-client

If that panics inside typify, bisect it:
  cargo xtask codex-schema bisect /tmp/codex-schema"
    );
}

/// Reads and parses a JSON file, with file-path context on failure.
pub(crate) fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {} as JSON", path.display()))
}

/// Parses the single positional `<gen-dir>` argument shared by the
/// `codex-schema regen` and `codex-schema bisect` subcommands: `--help`/`-h`
/// prints `usage` and exits 0 immediately, exactly one positional argument is
/// accepted as the generated-schema directory, and any further positional
/// argument is rejected. `usage` doubles as the error context when the
/// positional argument is missing.
pub(crate) fn parse_gen_dir(args: &[String], usage: &str) -> Result<PathBuf> {
    let mut gen_dir = None;
    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{usage}");
                std::process::exit(0);
            }
            other if gen_dir.is_none() => gen_dir = Some(PathBuf::from(other)),
            other => bail!("unexpected argument: {other}"),
        }
    }
    gen_dir.context(usage.to_string())
}

/// Reads the master + v2 schema bundles from `gen_dir`, merges them via
/// `merge::build_combined`, and extracts the flat `definitions` object.
/// Shared by `regen::regen` (which writes the combined schema out) and
/// `bisect::bisect` (which probes the definitions with typify).
pub(crate) fn load_combined_defs(gen_dir: &Path) -> Result<(Value, Map<String, Value>)> {
    let master = read_json(&gen_dir.join(MASTER_BUNDLE_FILE))?;
    let v2 = read_json(&gen_dir.join(V2_BUNDLE_FILE))?;

    let combined = merge::build_combined(&master, &v2)?;
    let combined_defs = combined
        .get("definitions")
        .and_then(Value::as_object)
        .context("combined schema missing \"definitions\"")?
        .clone();

    Ok((combined, combined_defs))
}
