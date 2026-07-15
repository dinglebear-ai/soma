use anyhow::{bail, Context, Result};
use walkdir::WalkDir;

use crate::{
    command_exists, generated_surfaces, mcp_registry, patterns, provider_manifest, run_cargo,
    run_cmd, scripts_lane_b, scripts_lane_d, test_siblings, web_source,
};

pub(crate) fn contract_audit() -> Result<()> {
    println!("==> contract-audit: local static/spec checks only");
    println!("==> [1/12] cargo xtask patterns");
    patterns::run(patterns::PatternOptions::default()).context("patterns contract check failed")?;

    println!("==> [2/12] cargo xtask check-test-siblings");
    test_siblings::check().context("test sibling check failed")?;

    println!("==> [3/12] cargo xtask check-docs");
    check_docs().context("generated docs check failed")?;

    println!("==> [4/12] cargo xtask check-stale-claims");
    check_stale_claims().context("stale claim check failed")?;

    println!("==> [5/12] cargo xtask check-schema-docs --check");
    scripts_lane_d::check_schema_docs(&["--check".to_owned()])
        .context("schema docs check failed")?;

    println!("==> [6/12] cargo xtask check-openapi --check");
    scripts_lane_d::check_openapi(&["--check".to_owned()]).context("OpenAPI docs check failed")?;

    println!("==> [7/12] cargo xtask check-mcp-registry");
    mcp_registry::check_default(std::path::Path::new("."))
        .context("MCP registry manifest check failed")?;

    println!("==> [8/12] cargo xtask check-provider-manifest-contract");
    provider_manifest::check().context("provider manifest contract check failed")?;

    println!("==> [9/12] cargo xtask check-palette-manifest --check");
    generated_surfaces::check_palette_manifest(&["--check".to_owned()])
        .context("Palette manifest check failed")?;

    println!("==> [10/12] cargo xtask generate-provider-surfaces --check");
    generated_surfaces::provider_surfaces(&["--check".to_owned()])
        .context("provider surfaces check failed")?;

    println!("==> [11/12] cargo xtask check-scaffold-intent-contract");
    scripts_lane_d::check_scaffold_intent_contract()
        .context("scaffold intent contract check failed")?;

    println!("==> [12/12] cargo xtask test-soma-features");
    scripts_lane_b::test_soma_features(std::path::Path::new("."))
        .context("Soma feature smoke failed")?;

    println!("==> contract-audit: passed; no live upstream services were contacted");
    Ok(())
}

pub(crate) fn generate_docs() -> Result<()> {
    run_cmd("python3", &["scripts/generate-docs.py", "--write"])
        .context("generated docs update failed")
}

pub(crate) fn check_docs() -> Result<()> {
    run_cmd("python3", &["scripts/generate-docs.py", "--check"])
        .context("generated docs are stale; run `cargo xtask generate-docs`")
}

pub(crate) fn check_stale_claims() -> Result<()> {
    run_cmd("python3", &["scripts/check-stale-claims.py"]).context("stale claim check failed")
}

pub(crate) fn dist() -> Result<()> {
    const BINARY_NAME: &str = "soma";

    println!("==> Building release binary: {BINARY_NAME}");
    run_cargo(&["build", "--release", "--locked", "--bin", BINARY_NAME])?;

    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
    let artifact = std::path::Path::new(&target_dir)
        .join("release")
        .join(BINARY_NAME);
    if !artifact.exists() {
        bail!("Release binary not found at {artifact:?} — build must have failed");
    }

    println!("==> Built {artifact:?}");
    println!("==> Run `just install-local` to install it to ~/.local/bin for plugin use");
    Ok(())
}

pub(crate) fn ci() -> Result<()> {
    println!("==> [1/13] cargo fmt --check");
    run_cargo(&["fmt", "--all", "--", "--check"]).context("fmt failed — run `cargo fmt` to fix")?;

    println!("==> [2/13] cargo clippy");
    run_cargo(&["clippy", "--all-targets", "--", "-D", "warnings"]).context("clippy failed")?;

    println!("==> [3/13] cargo nextest run --profile ci");
    if command_exists("cargo-nextest") {
        run_cargo(&["nextest", "run", "--profile", "ci"]).context("nextest failed")?;
    } else {
        eprintln!("  (nextest not installed — falling back to cargo test)");
        run_cargo(&["test"]).context("cargo test failed")?;
    }

    println!("==> [4/13] taplo check");
    if command_exists("taplo") {
        run_cmd("taplo", &["check"]).context("taplo check failed — run `taplo format` to fix")?;
    } else {
        eprintln!("  (taplo not installed — skipping TOML format check)");
    }

    println!("==> [5/13] cargo xtask patterns");
    patterns::run(patterns::PatternOptions::default())
        .context("PATTERNS.md contract check failed")?;

    println!("==> [6/13] cargo xtask check-test-siblings");
    test_siblings::check().context("test sibling check failed")?;

    println!("==> [7/13] cargo xtask check-docs");
    check_docs().context("generated docs check failed")?;

    println!("==> [8/13] cargo xtask check-stale-claims");
    check_stale_claims().context("stale claim check failed")?;

    println!("==> [9/13] cargo xtask check-mcp-registry");
    mcp_registry::check_default(std::path::Path::new("."))
        .context("MCP registry manifest check failed")?;

    println!("==> [10/13] cargo xtask check-provider-manifest-contract");
    provider_manifest::check().context("provider manifest contract check failed")?;

    println!("==> [11/13] cargo xtask check-palette-manifest --check");
    generated_surfaces::check_palette_manifest(&["--check".to_owned()])
        .context("Palette manifest check failed")?;

    println!("==> [12/13] cargo xtask check-web-source-sync");
    web_source::check().context("web source bundle drifted from apps/web")?;

    println!("==> [13/13] cargo audit");
    if command_exists("cargo-audit") {
        run_cargo(&["audit"]).context("cargo audit found vulnerabilities")?;
    } else {
        eprintln!(
            "  (cargo-audit not installed — skipping; install with `cargo install cargo-audit`)"
        );
    }

    println!("==> All CI checks passed!");
    Ok(())
}

pub(crate) fn symlink_docs() -> Result<()> {
    let mut created = 0usize;
    let mut skipped = 0usize;

    for entry in WalkDir::new(".")
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | "target")
        })
        .filter_map(|entry| entry.ok())
    {
        if entry.file_name() != "CLAUDE.md" {
            continue;
        }

        let dir = entry.path().parent().expect("CLAUDE.md has a parent");
        for link_name in ["AGENTS.md", "GEMINI.md"] {
            let link_path = dir.join(link_name);
            if link_path.exists() || link_path.symlink_metadata().is_ok() {
                println!("  skip  {}", link_path.display());
                skipped += 1;
                continue;
            }

            #[cfg(unix)]
            std::os::unix::fs::symlink("CLAUDE.md", &link_path)
                .with_context(|| format!("Failed to create symlink at {}", link_path.display()))?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file("CLAUDE.md", &link_path).with_context(|| {
                format!(
                    "Failed to create symlink at {} (may need developer mode on Windows)",
                    link_path.display()
                )
            })?;

            println!("  link  {} → CLAUDE.md", link_path.display());
            created += 1;
        }
    }

    println!("==> symlink-docs: {created} created, {skipped} already present");
    Ok(())
}

pub(crate) fn check_env() -> Result<()> {
    let required_vars: &[(&str, &str)] = &[];
    let optional_vars: &[(&str, &str)] = &[
        (
            "SOMA_MCP_TOKEN",
            "Static bearer token for /mcp (required in production; omit only in loopback dev mode)",
        ),
        (
            "SOMA_MCP_HOST",
            "Bind host (default 127.0.0.1 — set to 0.0.0.0 only with auth or trusted gateway)",
        ),
        ("SOMA_MCP_PORT", "Bind port (default 40060)"),
        (
            "RUST_LOG",
            "Log filter (e.g. info,rmcp=warn — default: info in server mode, warn in stdio/cli)",
        ),
    ];

    print_env_report(required_vars, optional_vars)
}

fn print_env_report(required_vars: &[(&str, &str)], optional_vars: &[(&str, &str)]) -> Result<()> {
    let mut missing = Vec::new();

    println!("==> Checking required environment variables:");
    for &(var, desc) in required_vars {
        match std::env::var(var) {
            Ok(v) if !v.is_empty() => println!("  OK  {var}"),
            _ => {
                println!("  MISSING  {var}");
                println!("           {desc}");
                missing.push(var);
            }
        }
    }

    println!("\n==> Optional variables (missing = feature degraded, not error):");
    for &(var, desc) in optional_vars {
        match std::env::var(var) {
            Ok(v) if !v.is_empty() => println!("  set      {var} = {v}"),
            _ => println!("  unset    {var}  ({desc})"),
        }
    }

    if !missing.is_empty() {
        bail!(
            "\nMissing required environment variables: {}\n\
             Copy .env.example to .env and fill in the values.",
            missing.join(", ")
        );
    }

    println!("\n==> All required environment variables are set.");
    Ok(())
}

pub(crate) fn print_help() {
    eprintln!("{HELP_TEXT}");
}

const HELP_TEXT: &str = "cargo xtask — repo automation for soma

USAGE:
  cargo xtask <command>

COMMANDS:
  dist                  Build release binary into Cargo target dir
  ci                    Run all CI checks: fmt, clippy, nextest, taplo, audit
  symlink-docs          Create AGENTS.md + GEMINI.md symlinks next to every CLAUDE.md
  check-env             Validate required environment variables are set
  check-test-siblings   Verify every src/*.rs has a sibling *_tests.rs
  patterns              Check static contracts from docs/PATTERNS.md (--strict, --json)
  contract-audit        Run local static/spec checks without live upstream calls
  scaffold             Plan/apply/verify a generated project from Soma
  codex-schema         Rebuild/bisect the vendored codex-app-server-client schema
                        (see `cargo xtask codex-schema --help`)
  cargo-generate        Smoke-test real cargo-generate output (--no-cargo-check)
  cargo-generate-post   Internal generated-project rewrite command
  generate-docs         Generate volatile docs and metadata from canonical specs
  check-docs            Validate generated docs and metadata are current
  check-mcp-registry    Validate server.json against docs/contracts/mcp-server.schema.json
  check-stale-claims    Fail on stale hardcoded Soma claims
  check-cargo-generate  Validate cargo-generate output
  sync-web-source       Copy apps/web into crates/soma-web/assets/source
  check-web-source-sync Validate bundled web source matches apps/web
  update-aurora-web     Refresh Aurora registry components, validate, then sync
  build-web             Build optional apps/web static export
  web-watch             Watch apps/web and rebuild on changes
  generate-cli          Generate dist/soma-cli through mcporter
  repair                Rebuild and restart local soma-mcp runtime
  test-mcp-auth         Smoke-test HTTP MCP bearer auth
  asciicheck            Check/fix explicit files for non-ASCII characters
  check-blob-size       Check changed git blobs against size budget
  block-env-commits     Prevent staged .env secrets from being committed
  check-coupled-files   Check common companion-file drift in a diff
  check-dependency-updates
                        Report Cargo dependency updates
  check-file-size       Check staged source files against size budgets
  check-openapi         Generate/check docs/generated/openapi.json
  check-plugin-hook-contract
                        Audit cross-repo plugin hook contracts
  run-ascii-check       Check or fix tracked source/config/docs ASCII hygiene
  check-plugin-stdio-smoke
                        Smoke-test installed plugin stdio binary
  check-runtime-current Check systemd/Docker runtime artifact freshness
  check-schema-docs     Generate/check docs/MCP_SCHEMA.md
  check-scaffold-intent-contract
                        Validate scaffold intent schema/examples
  apply-no-mcp-marketplace
                        Remove bundled MCP registrations for the no-MCP branch
  check-no-mcp-drift    Validate marketplace-no-MCP invariants and branch drift
  sync-cargo            Copy Cargo.lock into plugin data directories
  pre-release-check     Run release-readiness gate
  refresh-docs          Refresh ignored reference docs
  test-soma-features
                        Run Soma invariant smoke tests
  validate-plugin-layout
                        Validate Claude/Codex/Gemini plugin package layout
  check-version-sync    Validate release manifest version-file parity
  check-release-versions [--base REF] [--head REF] [--mode pr|main] [--json]
                        Validate changed release components have fresh versions/tags
  release-plan          Print changed release components and candidate tags
  sync-release-please-version
                        Sync version files to .release-please-manifest.json
  bump-version          Bump a component: cargo xtask bump-version soma patch
  bump-soma-version Bump Soma component: cargo xtask bump-soma-version patch
  changed-paths         Classify changed files into CI routing categories
  help                  Show this help

CUSTOMIZE:
  Add commands by extending the match block in xtask/src/main.rs.
  Keep dependencies minimal — xtask should compile in seconds.";

#[cfg(test)]
#[path = "workspace_commands_tests.rs"]
mod tests;
