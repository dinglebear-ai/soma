use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

use crate::{
    architecture, command_exists, generated_surfaces, mcp_registry, patterns, provider_manifest,
    run_cargo, run_cmd, scripts_lane_b, scripts_lane_d, test_siblings, web_source,
};

pub(crate) fn contract_audit() -> Result<()> {
    println!("==> contract-audit: local static/spec checks only");
    println!("==> [1/13] cargo xtask check-architecture");
    architecture::check(std::path::Path::new(".")).context("architecture check failed")?;

    println!("==> [2/13] cargo xtask patterns");
    patterns::run(patterns::PatternOptions::default()).context("patterns contract check failed")?;

    println!("==> [3/13] cargo xtask check-test-siblings");
    test_siblings::check().context("test sibling check failed")?;

    println!("==> [4/13] cargo xtask check-docs");
    check_docs().context("generated docs check failed")?;

    println!("==> [5/13] cargo xtask check-stale-claims");
    check_stale_claims().context("stale claim check failed")?;

    println!("==> [6/13] cargo xtask check-schema-docs --check");
    scripts_lane_d::check_schema_docs(&["--check".to_owned()])
        .context("schema docs check failed")?;

    println!("==> [7/13] cargo xtask check-openapi --check");
    scripts_lane_d::check_openapi(&["--check".to_owned()]).context("OpenAPI docs check failed")?;

    println!("==> [8/13] cargo xtask check-mcp-registry");
    mcp_registry::check_default(std::path::Path::new("."))
        .context("MCP registry manifest check failed")?;

    println!("==> [9/13] cargo xtask check-provider-manifest-contract");
    provider_manifest::check().context("provider manifest contract check failed")?;

    println!("==> [10/13] cargo xtask check-palette-manifest --check");
    generated_surfaces::check_palette_manifest(&["--check".to_owned()])
        .context("Palette manifest check failed")?;

    println!("==> [11/13] cargo xtask generate-provider-surfaces --check");
    generated_surfaces::provider_surfaces(&["--check".to_owned()])
        .context("provider surfaces check failed")?;

    println!("==> [12/13] cargo xtask check-scaffold-intent-contract");
    scripts_lane_d::check_scaffold_intent_contract()
        .context("scaffold intent contract check failed")?;

    println!("==> [13/13] cargo xtask test-soma-features");
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

/// `cargo xtask doc` — generate rustdoc API reference for workspace crates.
///
/// Defaults match the repo's documented doc-build posture: public items only,
/// no dependency docs, all features so the full gated surface renders. The
/// `--strict` flag enforces `RUSTDOCFLAGS=-D warnings` (what CI and
/// `cargo xtask ci` use); without it warnings are surfaced but non-fatal so a
/// stray doc-link doesn't block a local `cargo xtask doc --open`.
///
/// `--docsrs-cfg` appends `--cfg docsrs` to RUSTDOCFLAGS so the
/// `#![cfg_attr(docsrs, feature(doc_auto_cfg))]` attributes in feature-gated
/// crates activate and rustdoc renders per-item feature-requirement badges.
/// `doc_auto_cfg` is a nightly rustdoc feature, so this flag needs a nightly
/// toolchain (e.g. `RUSTUP_TOOLCHAIN=nightly cargo xtask doc --docsrs-cfg`);
/// the stable CI doc gate never passes it and is unaffected.
///
/// After a successful build the doc root also gets a landing `index.html`
/// (workspace crate listing) plus `openapi.html`/`openapi.json` (Redoc) — see
/// `doc_site::emit`.
pub(crate) fn doc(args: &[String]) -> Result<()> {
    let mut cargo_args = vec![
        "doc".to_owned(),
        "--workspace".to_owned(),
        "--no-deps".to_owned(),
        "--locked".to_owned(),
    ];
    let mut all_features = true;
    let mut open = false;
    let mut strict = false;
    let mut docsrs_cfg = false;
    let mut packages: Vec<String> = Vec::new();
    let mut document_private_items = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--open" => open = true,
            "--strict" => strict = true,
            "--docsrs-cfg" => docsrs_cfg = true,
            "--no-all-features" => all_features = false,
            "--document-private-items" => document_private_items = true,
            "--all-features" => all_features = true,
            "-p" | "--package" => {
                let pkg = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("`{arg}` requires a package name"))?;
                packages.push(pkg.clone());
            }
            "--help" | "-h" => {
                println!("{DOC_HELP}");
                return Ok(());
            }
            other => bail!("Unknown `cargo xtask doc` option: {other:?}"),
        }
    }

    if all_features {
        cargo_args.push("--all-features".to_owned());
    }
    if document_private_items {
        cargo_args.push("--document-private-items".to_owned());
    }
    for pkg in &packages {
        cargo_args.push("-p".to_owned());
        cargo_args.push(pkg.clone());
    }
    if open {
        cargo_args.push("--open".to_owned());
    }

    print_doc_plan(
        all_features,
        &packages,
        document_private_items,
        open,
        strict,
        docsrs_cfg,
    );

    // Drive `cargo doc` directly rather than through `run_cargo` so we can set
    // RUSTDOCFLAGS on the child env. Mirrors `run_cargo`'s streaming behavior.
    // Flags are appended to any RUSTDOCFLAGS already in the environment so an
    // operator's own flags (or docs.yml's global -D warnings) survive.
    let mut cmd = Command::new("cargo");
    cmd.args(&cargo_args);
    let mut rustdocflags: Vec<String> = std::env::var("RUSTDOCFLAGS")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .into_iter()
        .collect();
    if strict {
        rustdocflags.push("-D warnings".to_owned());
    }
    if docsrs_cfg {
        rustdocflags.push("--cfg docsrs".to_owned());
    }
    if !rustdocflags.is_empty() {
        cmd.env("RUSTDOCFLAGS", rustdocflags.join(" "));
    }
    cmd.stdin(Stdio::null());

    let status = cmd
        .status()
        .with_context(|| "Failed to spawn `cargo doc`")?;
    if !status.success() {
        bail!("`cargo doc` exited with status {status}");
    }

    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
    let doc_root = std::path::Path::new(&target_dir).join("doc");

    // Give the doc root an entry point: landing page + OpenAPI/Redoc assets.
    // This is what .github/workflows/docs.yml deploys to GitHub Pages.
    crate::doc_site::emit(&doc_root)?;

    println!();
    println!("==> Rustdoc generated under {doc_root:?}");
    if !open {
        println!(
            "    Open {}/index.html (or pass --open)",
            doc_root.display()
        );
    }
    if !strict {
        println!("    Note: warnings were not fatal. CI enforces them via `--strict`.");
    }
    Ok(())
}

/// Build rustdoc with `RUSTDOCFLAGS=-D warnings` — the CI-grade doc gate.
/// Used by `cargo xtask ci`. Standalone `cargo xtask doc --strict` uses the
/// same flag via the `doc()` arg path; this helper exists so `ci()` reads as
/// one line per check, matching the surrounding steps.
fn run_doc_strict() -> Result<()> {
    let status = Command::new("cargo")
        .args([
            "doc",
            "--workspace",
            "--no-deps",
            "--all-features",
            "--locked",
        ])
        .env("RUSTDOCFLAGS", "-D warnings")
        .stdin(Stdio::null())
        .status()
        .context("Failed to spawn `cargo doc`")?;
    if !status.success() {
        bail!("`cargo doc` exited with status {status}");
    }
    Ok(())
}

fn print_doc_plan(
    all_features: bool,
    packages: &[String],
    document_private_items: bool,
    open: bool,
    strict: bool,
    docsrs_cfg: bool,
) {
    println!("==> cargo xtask doc");
    println!(
        "    features:        {}",
        if all_features { "all" } else { "default" }
    );
    println!(
        "    packages:        {}",
        if packages.is_empty() {
            "workspace".to_owned()
        } else {
            packages.join(", ")
        }
    );
    println!(
        "    private items:   {}",
        if document_private_items {
            "yes"
        } else {
            "no (public only)"
        }
    );
    println!("    open in browser: {}", if open { "yes" } else { "no" });
    println!(
        "    strict (-D warn): {}",
        if strict { "yes" } else { "no" }
    );
    println!(
        "    docsrs cfg:      {}",
        if docsrs_cfg {
            "yes (--cfg docsrs; needs nightly rustdoc)"
        } else {
            "no"
        }
    );
}

const DOC_HELP: &str = "cargo xtask doc — generate Rust API documentation (rustdoc)

USAGE:
  cargo xtask doc [OPTIONS]

OPTIONS:
      --open                    Open the generated docs in a browser
      --strict                  Treat rustdoc warnings as errors
                                (RUSTDOCFLAGS=\"-D warnings\"; mirrors CI)
      --docsrs-cfg              Append `--cfg docsrs` to RUSTDOCFLAGS so
                                feature-requirement badges render
                                (doc_auto_cfg; requires a nightly toolchain)
      --no-all-features         Document default features only (faster)
      --all-features            Document all features (default)
      --document-private-items  Include private items (internal/team docs)
  -p, --package <NAME>          Document a single workspace package
                                (repeatable; --no-deps is always set)
  -h, --help                    Show this help

DEFAULTS:
  Public items only, no dependency docs, all features. These match the
  repo's documented cargo-doc posture and what `.github/workflows/docs.yml`
  deploys to GitHub Pages. Every run also writes target/doc/index.html (a
  landing page listing all workspace crates) plus openapi.html/openapi.json
  (the REST contract rendered with Redoc).

EXAMPLES:
  cargo xtask doc                     # full workspace API docs
  cargo xtask doc --open              # ...and open in a browser
  cargo xtask doc -p soma-application # one crate only
  cargo xtask doc --strict            # CI-grade (warnings are errors)
  RUSTUP_TOOLCHAIN=nightly cargo xtask doc --docsrs-cfg  # feature badges";

pub(crate) fn ci() -> Result<()> {
    println!("==> [1/15] cargo fmt --check");
    run_cargo(&["fmt", "--all", "--", "--check"]).context("fmt failed — run `cargo fmt` to fix")?;

    println!("==> [2/15] cargo xtask check-architecture");
    architecture::check(std::path::Path::new(".")).context("architecture check failed")?;

    println!("==> [3/15] cargo clippy");
    run_cargo(&["clippy", "--all-targets", "--", "-D", "warnings"]).context("clippy failed")?;

    println!("==> [4/15] cargo doc --workspace --no-deps --all-features (-D warnings)");
    run_doc_strict().context("rustdoc failed — run `cargo xtask doc --strict` to see details")?;

    println!("==> [5/15] cargo nextest run --profile ci");
    if command_exists("cargo-nextest") {
        run_cargo(&["nextest", "run", "--profile", "ci"]).context("nextest failed")?;
    } else {
        eprintln!("  (nextest not installed — falling back to cargo test)");
        run_cargo(&["test"]).context("cargo test failed")?;
    }

    println!("==> [6/15] taplo check");
    if command_exists("taplo") {
        run_cmd("taplo", &["check"]).context("taplo check failed — run `taplo format` to fix")?;
    } else {
        eprintln!("  (taplo not installed — skipping TOML format check)");
    }

    println!("==> [7/15] cargo xtask patterns");
    patterns::run(patterns::PatternOptions::default())
        .context("PATTERNS.md contract check failed")?;

    println!("==> [8/15] cargo xtask check-test-siblings");
    test_siblings::check().context("test sibling check failed")?;

    println!("==> [9/15] cargo xtask check-docs");
    check_docs().context("generated docs check failed")?;

    println!("==> [10/15] cargo xtask check-stale-claims");
    check_stale_claims().context("stale claim check failed")?;

    println!("==> [11/15] cargo xtask check-mcp-registry");
    mcp_registry::check_default(std::path::Path::new("."))
        .context("MCP registry manifest check failed")?;

    println!("==> [12/15] cargo xtask check-provider-manifest-contract");
    provider_manifest::check().context("provider manifest contract check failed")?;

    println!("==> [13/15] cargo xtask check-palette-manifest --check");
    generated_surfaces::check_palette_manifest(&["--check".to_owned()])
        .context("Palette manifest check failed")?;

    println!("==> [14/15] cargo xtask check-web-source-sync");
    web_source::check().context("web source bundle drifted from apps/web")?;

    println!("==> [15/15] cargo audit");
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
  check-architecture    Validate workspace dependency-layer boundaries
  check-test-siblings   Verify every src/*.rs has a sibling *_tests.rs
  patterns              Check static contracts from docs/PATTERNS.md (--strict, --json)
  contract-audit        Run local static/spec checks without live upstream calls
  scaffold             Plan/apply/verify a generated project from Soma
  codex-schema         Rebuild/bisect the vendored codex-app-server-client schema
                        (see `cargo xtask codex-schema --help`)
  cargo-generate        Smoke-test real cargo-generate output (--no-cargo-check)
  cargo-generate-post   Internal generated-project rewrite command
  generate-docs         Generate volatile docs and metadata from canonical specs
  doc                   Generate Rust API docs (rustdoc) for workspace crates (--open, --strict)
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
  test-trace-headers    Bounded live smoke for SOMA_MCP_TRACE_HEADERS
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
  check-ts-client       Regenerate/verify the checked-in codex-app-server-client TS REST client
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
