//! xtask — Repo automation for soma.
//!
//! Invoked via: `cargo xtask <command>`
//!
//! Commands:
//!   dist         Build release binary into Cargo target dir
//!   ci           Run all CI checks: fmt, clippy, nextest, taplo, audit
//!   symlink-docs Create AGENTS.md and GEMINI.md symlinks next to every CLAUDE.md
//!   check-env    Validate required environment variables are set
//!   check-architecture Validate workspace dependency-layer boundaries
//!   patterns     Check static contracts from docs/PATTERNS.md
//!   contract-audit Run local static/spec checks for REST-client MCP servers
//!   scaffold     Plan, generate, or verify a new project from Soma
//!   codex-schema Rebuild/bisect/diff the vendored codex-app-server-client schema
//!   cargo-generate Smoke-test cargo-generate output
//!   cargo-generate-post Apply cargo-generate post-processing rewrites
//!   generate-docs Generate volatile docs and metadata from canonical specs
//!   doc           Generate Rust API documentation (rustdoc) for workspace crates
//!   generate-provider-surfaces Generate provider docs and marketplace catalogs
//!   check-docs    Validate generated docs and metadata are current
//!   check-mcp-registry Validate server.json against the MCP registry schema
//!   check-stale-claims Fail on stale hardcoded Soma claims
//!   sync-web-source Copy apps/web into the bundled soma-web scaffold source
//!   check-web-source-sync Validate bundled web source matches apps/web
//!   update-aurora-web Refresh Aurora components, validate apps/web, then sync bundle
//!   block-env-commits Prevent staged .env secrets from being committed
//!   check-coupled-files Check common companion-file drift in a diff
//!   check-file-size Check staged source files against size budgets
//!   run-ascii-check Check or fix tracked source/config/docs ASCII hygiene
//!   check-plugin-stdio-smoke Smoke-test installed plugin stdio binary
//!   apply-no-mcp-marketplace Apply deterministic no-MCP marketplace branch transform
//!   check-no-mcp-drift Validate marketplace-no-mcp branch invariants and drift
//!   check-ts-client Regenerate/verify the checked-in codex-app-server-client TypeScript REST client
//!   sync-cargo   Copy Cargo.lock into plugin data directories
//!   check-release-versions Validate release component version policy
//!   release-plan Print changed release components and candidate tags
//!   sync-release-please-version Sync release files to .release-please-manifest.json
//!   bump-version Bump a release component version
//!   changed-paths Classify changed files into CI routing categories
//!
//! CUSTOMIZE: Add your own commands by adding arms to the match block below.
//!           Keep each command as a separate `fn` for readability.
//!
//! Philosophy: xtask replaces ad-hoc shell scripts. It gets type-checked by the
//! compiler, works cross-platform, and is easy to extend. Keep functions small
//! and use `std::process::Command` to shell out to existing tools rather than
//! reimplementing them in Rust.

use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

mod architecture;
mod architecture_graph;
mod cargo_generate;
mod cargo_generate_post;
mod ci_paths;
mod codex_schema;
mod doc_site;
mod generated_surfaces;
mod mcp_registry;
mod no_mcp;
mod patterns;
mod provider_manifest;
mod release_commands;
mod release_versions;
mod rmcp_release_monitor;
mod scaffold;
mod scripts;
mod scripts_lane_a;
mod scripts_lane_b;
mod scripts_lane_c;
mod scripts_lane_d;
mod test_siblings;
mod trace_headers_smoke;
mod ts_client;
mod web_source;
mod workspace_commands;

fn main() -> Result<()> {
    // Cargo sets CARGO_MANIFEST_DIR for the workspace root when invoked as
    // `cargo xtask`. Change into the workspace root so all commands work
    // regardless of the cwd from which the user invoked cargo.
    //
    // CUSTOMIZE: This path navigation assumes xtask/ is a direct child of the
    //           workspace root. If you restructure, adjust the `..` accordingly.
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml must have a parent directory");
    std::env::set_current_dir(workspace_root).context("Failed to change into workspace root")?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("dist") => workspace_commands::dist(),
        Some("ci") => workspace_commands::ci(),
        Some("symlink-docs") => workspace_commands::symlink_docs(),
        Some("check-env") => workspace_commands::check_env(),
        Some("patterns") => patterns_cmd(&args[1..]),
        Some("contract-audit") => workspace_commands::contract_audit(),
        Some("scaffold") => scaffold::run(&args[1..]),
        Some("codex-schema") => codex_schema::run(&args[1..]),
        Some("cargo-generate") => cargo_generate(&args[1..]),
        Some("cargo-generate-post") => cargo_generate_post::run(&args[1..]),
        Some("generate-docs") => workspace_commands::generate_docs(),
        Some("doc") => workspace_commands::doc(&args[1..]),
        Some("generate-provider-surfaces") => generated_surfaces::provider_surfaces(&args[1..]),
        Some("check-docs") => workspace_commands::check_docs(),
        Some("check-architecture") => architecture::check(workspace_root),
        Some("check-mcp-registry") => mcp_registry::check_cmd(workspace_root, &args[1..]),
        Some("check-stale-claims") => workspace_commands::check_stale_claims(),
        Some("check-cargo-generate") => scripts_lane_d::check_cargo_generate(&args[1..]),
        Some("sync-web-source") => web_source::sync(),
        Some("check-web-source-sync") => web_source::check(),
        Some("update-aurora-web") => web_source::update_aurora(),
        Some("build-web") => scripts_lane_a::build_web(),
        Some("web-watch") => scripts_lane_a::web_watch(),
        Some("generate-cli") => scripts_lane_a::generate_cli(),
        Some("repair") => scripts_lane_a::repair(),
        Some("test-mcp-auth") => scripts_lane_a::test_mcp_auth(&args[1..]),
        Some("test-trace-headers") => trace_headers_smoke::test_trace_headers(&args[1..]),
        Some("block-env-commits") => scripts::block_env_commits(),
        Some("asciicheck") => scripts_lane_d::asciicheck(&args[1..]),
        Some("check-blob-size") => scripts_lane_c::check_blob_size(&args[1..]),
        Some("check-coupled-files") => scripts::check_coupled_files(&args[1..]),
        Some("check-dependency-updates") => scripts_lane_c::check_dependency_updates(&args[1..]),
        Some("check-file-size") => scripts::check_file_size(),
        Some("check-openapi") => scripts_lane_d::check_openapi(&args[1..]),
        Some("check-openapi-drift") => scripts_lane_d::check_openapi(&args[1..]),
        Some("check-ts-client") => ts_client::run(&args[1..]),
        Some("check-palette-manifest") => generated_surfaces::check_palette_manifest(&args[1..]),
        Some("check-provider-manifest-contract") => provider_manifest::check(),
        Some("check-plugin-hook-contract") => {
            scripts_lane_c::check_plugin_hook_contract(&args[1..])
        }
        Some("run-ascii-check") => scripts::run_ascii_check(&args[1..]),
        Some("check-plugin-stdio-smoke") => scripts::check_plugin_stdio_smoke(),
        Some("check-runtime-current") => scripts_lane_c::check_runtime_current(&args[1..]),
        Some("check-schema-docs") => scripts_lane_d::check_schema_docs(&args[1..]),
        Some("check-scaffold-intent-contract") => scripts_lane_d::check_scaffold_intent_contract(),
        Some("apply-no-mcp-marketplace") => no_mcp::apply_cmd(),
        Some("check-no-mcp-drift") => no_mcp::check_cmd(&args[1..]),
        Some("sync-cargo") => scripts::sync_cargo(),
        Some("pre-release-check") => scripts_lane_b::pre_release_check(&args[1..]),
        Some("refresh-docs") => scripts_lane_c::refresh_docs(&args[1..]),
        Some("test-soma-features") => scripts_lane_b::test_soma_features(workspace_root),
        Some("validate-plugin-layout") => {
            let plugin_root = std::env::var_os("PLUGIN_ROOT").map(std::path::PathBuf::from);
            scripts_lane_b::validate_plugin_layout(workspace_root, plugin_root.as_deref())
        }
        Some("check-test-siblings") => test_siblings::check(),
        Some("check-version-sync") => {
            scripts_lane_b::check_version_sync(workspace_root, &args[1..])
        }
        Some("check-release-versions") => release_commands::check(workspace_root, &args[1..]),
        Some("release-plan") => release_commands::plan(workspace_root, &args[1..]),
        Some("sync-release-please-version") => {
            release_versions::sync_release_please_version(workspace_root, "soma")
        }
        Some("rmcp-release-monitor") => rmcp_release_monitor::run(&args[1..]),
        Some("bump-version") => release_commands::bump(workspace_root, &args[1..]),
        Some("bump-soma-version") => scripts_lane_b::bump_version(workspace_root, &args[1..]),
        Some("changed-paths") => ci_paths::run(&args[1..]),
        Some("--help") | Some("-h") | Some("help") | None => {
            workspace_commands::print_help();
            Ok(())
        }
        Some(unknown) => {
            bail!("Unknown xtask command: {unknown:?}\nRun `cargo xtask --help` for usage.")
        }
    }
}

// =============================================================================
// cargo-generate — Smoke-test generated scaffold output
// =============================================================================

fn cargo_generate(args: &[String]) -> Result<()> {
    cargo_generate::run(args)
}

// =============================================================================
// patterns — Check docs/PATTERNS.md contracts
// =============================================================================

fn patterns_cmd(args: &[String]) -> Result<()> {
    let mut options = patterns::PatternOptions::default();
    for arg in args {
        match arg.as_str() {
            "--strict" => options.strict = true,
            "--json" => options.json = true,
            "--help" | "-h" => {
                println!("Usage: cargo xtask patterns [--strict] [--json]");
                return Ok(());
            }
            unknown => bail!("Unknown patterns option: {unknown}"),
        }
    }
    patterns::run(options)
}

// =============================================================================
// Helpers
// =============================================================================

/// Run a `cargo` subcommand, forwarding stdout/stderr.
pub(crate) fn run_cargo(args: &[&str]) -> Result<()> {
    run_cmd("cargo", args)
}

/// Run an arbitrary command, forwarding stdout/stderr. Fails if exit code != 0.
pub(crate) fn run_cmd(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("Failed to spawn `{program}`"))?;
    if !status.success() {
        bail!("`{program} {}` exited with status {status}", args.join(" "));
    }
    Ok(())
}

/// Run an arbitrary command and return stdout. Fails if exit code != 0.
pub(crate) fn run_cmd_output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Failed to spawn `{program}`"))?;
    if !output.status.success() {
        bail!(
            "`{program} {}` exited with status {}",
            args.join(" "),
            output.status
        );
    }
    String::from_utf8(output.stdout)
        .with_context(|| format!("`{program}` emitted non-UTF-8 stdout"))
}

/// Check whether a cargo subcommand (or standalone binary) is installed.
///
/// Checks for both `cargo-nextest` (cargo subcommand) and `nextest` in PATH.
pub(crate) fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
