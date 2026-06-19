//! xtask — Repo automation for rmcp-template.
//!
//! Invoked via: `cargo xtask <command>`
//!
//! Commands:
//!   dist         Build release binary into Cargo target dir
//!   ci           Run all CI checks: fmt, clippy, nextest, taplo, audit
//!   symlink-docs Create AGENTS.md and GEMINI.md symlinks next to every CLAUDE.md
//!   check-env    Validate required environment variables are set
//!   patterns     Check static contracts from docs/PATTERNS.md
//!   contract-audit Run local static/spec checks for REST-client MCP servers
//!   cargo-generate Smoke-test cargo-generate output
//!   cargo-generate-post Apply cargo-generate post-processing rewrites
//!   generate-docs Generate volatile docs and metadata from canonical specs
//!   check-docs    Validate generated docs and metadata are current
//!   check-stale-claims Fail on stale hardcoded template claims
//!   sync-web-source Copy apps/web into the bundled rtemplate-web scaffold source
//!   check-web-source-sync Validate bundled web source matches apps/web
//!   update-aurora-web Refresh Aurora components, validate apps/web, then sync bundle
//!   block-env-commits Prevent staged .env secrets from being committed
//!   check-coupled-files Check common companion-file drift in a diff
//!   check-file-size Check staged source files against size budgets
//!   run-ascii-check Check or fix tracked source/config/docs ASCII hygiene
//!   check-plugin-stdio-smoke Smoke-test installed plugin stdio binary
//!   sync-cargo   Copy Cargo.lock into plugin data directories
//!   check-release-versions Validate release component version policy
//!   release-plan Print changed release components and candidate tags
//!   bump-version Bump a release component version
//!
//! TEMPLATE: Add your own commands by adding arms to the match block below.
//!           Keep each command as a separate `fn` for readability.
//!
//! Philosophy: xtask replaces ad-hoc shell scripts. It gets type-checked by the
//! compiler, works cross-platform, and is easy to extend. Keep functions small
//! and use `std::process::Command` to shell out to existing tools rather than
//! reimplementing them in Rust.

use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

mod cargo_generate;
mod cargo_generate_post;
mod patterns;
mod release_versions;
mod scripts;
mod web_source;

fn main() -> Result<()> {
    // Cargo sets CARGO_MANIFEST_DIR for the workspace root when invoked as
    // `cargo xtask`. Change into the workspace root so all commands work
    // regardless of the cwd from which the user invoked cargo.
    //
    // TEMPLATE: This path navigation assumes xtask/ is a direct child of the
    //           workspace root. If you restructure, adjust the `..` accordingly.
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml must have a parent directory");
    std::env::set_current_dir(workspace_root).context("Failed to change into workspace root")?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("dist") => dist(),
        Some("ci") => ci(),
        Some("symlink-docs") => symlink_docs(),
        Some("check-env") => check_env(),
        Some("patterns") => patterns_cmd(&args[1..]),
        Some("contract-audit") => contract_audit(),
        Some("cargo-generate") => cargo_generate(&args[1..]),
        Some("cargo-generate-post") => cargo_generate_post::run(&args[1..]),
        Some("generate-docs") => generate_docs(),
        Some("check-docs") => check_docs(),
        Some("check-stale-claims") => check_stale_claims(),
        Some("sync-web-source") => web_source::sync(),
        Some("check-web-source-sync") => web_source::check(),
        Some("update-aurora-web") => web_source::update_aurora(),
        Some("block-env-commits") => scripts::block_env_commits(),
        Some("check-coupled-files") => scripts::check_coupled_files(&args[1..]),
        Some("check-file-size") => scripts::check_file_size(),
        Some("run-ascii-check") => scripts::run_ascii_check(&args[1..]),
        Some("check-plugin-stdio-smoke") => scripts::check_plugin_stdio_smoke(),
        Some("sync-cargo") => scripts::sync_cargo(),
        Some("check-test-siblings") => check_test_siblings(),
        Some("check-version-sync") => release_versions::check_version_sync(workspace_root),
        Some("check-release-versions") => check_release_versions_cmd(workspace_root, &args[1..]),
        Some("release-plan") => release_plan_cmd(workspace_root, &args[1..]),
        Some("bump-version") => bump_version_cmd(workspace_root, &args[1..]),
        Some("--help") | Some("-h") | Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(unknown) => {
            bail!("Unknown xtask command: {unknown:?}\nRun `cargo xtask --help` for usage.")
        }
    }
}

fn check_release_versions_cmd(root: &std::path::Path, args: &[String]) -> Result<()> {
    let options = ReleaseCommandOptions::parse(args)?;
    release_versions::check(
        root,
        options.base.as_deref(),
        &options.head,
        options.mode,
        options.json,
    )
}

fn release_plan_cmd(root: &std::path::Path, args: &[String]) -> Result<()> {
    let options = ReleaseCommandOptions::parse(args)?;
    let plans = release_versions::plan(root, options.base.as_deref(), &options.head, options.mode)?;
    release_versions::print_plans(&plans, options.json)
}

fn bump_version_cmd(root: &std::path::Path, args: &[String]) -> Result<()> {
    if args.len() != 2 {
        bail!("Usage: cargo xtask bump-version <component> <patch|minor|major>");
    }
    let level = parse_bump_level(&args[1])?;
    release_versions::bump(root, &args[0], level)
}

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

// =============================================================================
// cargo-generate — Smoke-test generated template output
// =============================================================================

fn cargo_generate(args: &[String]) -> Result<()> {
    cargo_generate::run(args)
}

// =============================================================================
// contract-audit — Safe local contract/spec checks for REST-client MCP servers
// =============================================================================

/// Run the local, non-destructive audit suite for the template contract.
///
/// This command intentionally avoids live upstream services. REST-client
/// behavior belongs in per-server mock-upstream tests; this command verifies
/// the static contract surfaces that every derived server should keep current.
fn contract_audit() -> Result<()> {
    println!("==> contract-audit: local static/spec checks only");
    println!("==> [1/7] cargo xtask patterns");
    patterns::run(patterns::PatternOptions::default()).context("patterns contract check failed")?;

    println!("==> [2/7] cargo xtask check-test-siblings");
    check_test_siblings().context("test sibling check failed")?;

    println!("==> [3/7] cargo xtask check-docs");
    check_docs().context("generated docs check failed")?;

    println!("==> [4/8] cargo xtask check-stale-claims");
    check_stale_claims().context("stale claim check failed")?;

    println!("==> [5/8] scripts/check-schema-docs.py --check");
    run_cmd("python3", &["scripts/check-schema-docs.py", "--check"])
        .context("schema docs check failed")?;

    println!("==> [6/8] scripts/check-openapi.py --check");
    run_cmd("python3", &["scripts/check-openapi.py", "--check"])
        .context("OpenAPI docs check failed")?;

    println!("==> [7/8] scripts/check-scaffold-intent-contract.py");
    run_cmd("python3", &["scripts/check-scaffold-intent-contract.py"])
        .context("scaffold intent contract check failed")?;

    println!("==> [8/8] scripts/test-template-features.sh");
    run_cmd("bash", &["scripts/test-template-features.sh"])
        .context("template feature smoke failed")?;

    println!("==> contract-audit: passed; no live upstream services were contacted");
    Ok(())
}

// =============================================================================
// generated docs — Render/check volatile docs and metadata
// =============================================================================

fn generate_docs() -> Result<()> {
    run_cmd("python3", &["scripts/generate-docs.py", "--write"])
        .context("generated docs update failed")
}

fn check_docs() -> Result<()> {
    run_cmd("python3", &["scripts/generate-docs.py", "--check"])
        .context("generated docs are stale; run `cargo xtask generate-docs`")
}

fn check_stale_claims() -> Result<()> {
    run_cmd("python3", &["scripts/check-stale-claims.py"]).context("stale claim check failed")
}

// =============================================================================
// dist — Build release binary
// =============================================================================

/// Build the release binary. Distribution is handled by package/release tooling;
/// plugins reference an installed PATH binary and do not bundle artifacts.
///
/// TEMPLATE: Replace "rtemplate" with your binary name throughout this function.
///           The binary name must match Cargo.toml `[[bin]] name = "..."`.
fn dist() -> Result<()> {
    // TEMPLATE: Replace "rtemplate" with your binary name.
    const BINARY_NAME: &str = "rtemplate";

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

// =============================================================================
// ci — Run all CI checks locally
// =============================================================================

/// Run all CI checks in sequence: fmt, clippy, nextest, taplo, audit.
///
/// This mirrors what `.github/workflows/ci.yml` runs. Use it to catch failures
/// before pushing.
///
/// TEMPLATE: Add or remove steps to match your CI pipeline.
fn ci() -> Result<()> {
    println!("==> [1/9] cargo fmt --check");
    run_cargo(&["fmt", "--all", "--", "--check"]).context("fmt failed — run `cargo fmt` to fix")?;

    println!("==> [2/9] cargo clippy");
    run_cargo(&["clippy", "--all-targets", "--", "-D", "warnings"]).context("clippy failed")?;

    println!("==> [3/9] cargo nextest run --profile ci");
    // Falls back to cargo test if nextest isn't installed.
    // TEMPLATE: Remove the fallback once nextest is in your CI environment.
    if command_exists("cargo-nextest") {
        run_cargo(&["nextest", "run", "--profile", "ci"]).context("nextest failed")?;
    } else {
        eprintln!("  (nextest not installed — falling back to cargo test)");
        run_cargo(&["test"]).context("cargo test failed")?;
    }

    println!("==> [4/9] taplo check");
    // TEMPLATE: Remove this step if you don't use taplo.
    if command_exists("taplo") {
        run_cmd("taplo", &["check"]).context("taplo check failed — run `taplo format` to fix")?;
    } else {
        eprintln!("  (taplo not installed — skipping TOML format check)");
    }

    println!("==> [5/9] cargo xtask patterns");
    patterns::run(patterns::PatternOptions::default())
        .context("PATTERNS.md contract check failed")?;

    println!("==> [6/9] cargo xtask check-test-siblings");
    check_test_siblings().context("test sibling check failed")?;

    println!("==> [7/9] cargo xtask check-docs");
    check_docs().context("generated docs check failed")?;

    println!("==> [8/10] cargo xtask check-stale-claims");
    check_stale_claims().context("stale claim check failed")?;

    println!("==> [9/10] cargo xtask check-web-source-sync");
    web_source::check().context("web source bundle drifted from apps/web")?;

    println!("==> [10/10] cargo audit");
    // TEMPLATE: Remove if you don't want advisory audits in local CI.
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

// =============================================================================
// check-test-siblings — Verify every src/*.rs has a sibling *_tests.rs
// =============================================================================

/// Walk crate `src/` trees and report any `.rs` file missing a sibling
/// `{stem}_tests.rs`.
///
/// Files excluded from the check:
///   - Files whose name already ends in `_tests.rs` (they ARE the test sibling)
///   - `main.rs` and `lib.rs` (entry points with no business logic to unit-test)
///
/// Exits non-zero if any sibling is missing, so it can gate CI.
fn check_test_siblings() -> Result<()> {
    const EXEMPT: &[&str] = &["main.rs", "lib.rs"];
    const ORPHAN_EXEMPT: &[&str] = &["cli_tests.rs", "mcp_tests.rs"];

    let mut missing: Vec<std::path::PathBuf> = Vec::new();

    for root in crate_src_roots() {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            if !name.ends_with(".rs") || name.ends_with("_tests.rs") || EXEMPT.contains(&name) {
                continue;
            }

            let stem = name.strip_suffix(".rs").unwrap();
            let sibling = path.parent().unwrap().join(format!("{stem}_tests.rs"));

            if !sibling.exists() {
                missing.push(path.to_owned());
            }
        }
    }

    // Reverse check: _tests.rs files with no corresponding source are orphans.
    let mut orphans: Vec<std::path::PathBuf> = Vec::new();
    for root in crate_src_roots() {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if !name.ends_with("_tests.rs") {
                continue;
            }
            if ORPHAN_EXEMPT.contains(&name) {
                continue;
            }
            let stem = name.strip_suffix("_tests.rs").unwrap();
            let source = path.parent().unwrap().join(format!("{stem}.rs"));
            if !source.exists() {
                orphans.push(path.to_owned());
            }
        }
    }

    let ok = missing.is_empty() && orphans.is_empty();

    if !missing.is_empty() {
        println!(
            "==> check-test-siblings: missing _tests.rs siblings ({}):",
            missing.len()
        );
        for path in &missing {
            let stem = path.file_stem().unwrap().to_string_lossy();
            println!(
                "  MISSING  {}  (expected {}_tests.rs)",
                path.display(),
                stem
            );
        }
    }
    if !orphans.is_empty() {
        println!(
            "==> check-test-siblings: orphaned _tests.rs files ({}):",
            orphans.len()
        );
        for path in &orphans {
            println!("  ORPHAN   {}  (no matching source file)", path.display());
        }
    }
    if ok {
        println!("==> check-test-siblings: all source files have a _tests.rs sibling");
        return Ok(());
    }
    bail!("{} missing, {} orphaned", missing.len(), orphans.len());
}

fn crate_src_roots() -> Vec<std::path::PathBuf> {
    [
        "crates/rmcp-template/src",
        "crates/rtemplate-api/src",
        "crates/rtemplate-cli/src",
        "crates/rtemplate-contracts/src",
        "crates/rtemplate-mcp/src",
        "crates/rtemplate-observability/src",
        "crates/rtemplate-runtime/src",
        "crates/rtemplate-service/src",
        "crates/rtemplate-web/src",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .collect()
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
// symlink-docs — Create AGENTS.md + GEMINI.md symlinks next to every CLAUDE.md
// =============================================================================

/// Walk the repo and create sibling symlinks next to every CLAUDE.md found.
///
/// Pattern §32: CLAUDE.md is the single source of truth. AGENTS.md (Codex/OpenAI)
/// and GEMINI.md (Google) are symlinks so all AI systems read the same instructions.
///
/// This applies to ALL CLAUDE.md files in the repo, not just the root — nested
/// CLAUDE.md files in plugins/, xtask/, etc. all get symlinks.
///
/// TEMPLATE: No changes needed here — this works for any repo using CLAUDE.md.
///
/// Run after adding any new CLAUDE.md file:
///   cargo xtask symlink-docs
fn symlink_docs() -> Result<()> {
    let mut created = 0usize;
    let mut skipped = 0usize;

    // Walk the full repo, skipping .git/ and target/ (not real project dirs)
    for entry in WalkDir::new(".")
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            // Skip .git and target — they're not repo source dirs
            !matches!(name.as_ref(), ".git" | "target")
        })
        .filter_map(|e| e.ok())
    {
        if entry.file_name() != "CLAUDE.md" {
            continue;
        }

        let claude_path = entry.path();
        let dir = claude_path
            .parent()
            .expect("CLAUDE.md must be inside a directory");

        // Create sibling symlinks: AGENTS.md → CLAUDE.md, GEMINI.md → CLAUDE.md
        // Both use a relative target so they remain valid after `git clone`.
        for link_name in ["AGENTS.md", "GEMINI.md"] {
            let link_path = dir.join(link_name);

            if link_path.exists() || link_path.symlink_metadata().is_ok() {
                // Already exists (or is a dangling symlink) — skip
                println!("  skip  {}", link_path.display());
                skipped += 1;
                continue;
            }

            // Symlink target is always relative: "CLAUDE.md" → sibling file
            #[cfg(unix)]
            std::os::unix::fs::symlink("CLAUDE.md", &link_path)
                .with_context(|| format!("Failed to create symlink at {}", link_path.display()))?;

            // Windows: create a file symlink (requires developer mode or admin)
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

// =============================================================================
// check-env — Validate required environment variables
// =============================================================================

/// Validate that all required environment variables are set and non-empty.
///
/// Run this to get a clear error message before starting the server, rather
/// than a cryptic runtime failure.
///
/// TEMPLATE: Replace the variable names in REQUIRED_VARS with your service's
///           actual required environment variables.
///
/// Variables listed as "optional" are checked for presence but not required —
/// the server will start without them but some features may be unavailable.
fn check_env() -> Result<()> {
    // TEMPLATE: Add or remove required variables for your service.
    //   Format: (&str, &str)  →  (ENV_VAR_NAME, "description of what it's for")
    //
    // The template's ExampleClient doesn't require API credentials to boot
    // (stub mode works without them). Your real service likely does — update
    // REQUIRED_VARS accordingly.
    const REQUIRED_VARS: &[(&str, &str)] = &[
        // TEMPLATE: Uncomment and adapt once you have a real upstream service:
        // ("RTEMPLATE_API_URL", "Full base URL of the upstream service (e.g. https://api.example.com/v1)"),
        // ("RTEMPLATE_API_KEY", "API key or bearer token for the upstream service"),
    ];

    // TEMPLATE: Optional variables — server boots without them but warns.
    const OPTIONAL_VARS: &[(&str, &str)] = &[
        (
            "RTEMPLATE_MCP_TOKEN",
            "Static bearer token for /mcp (required in production; omit only in loopback dev mode)",
        ),
        (
            "RTEMPLATE_MCP_HOST",
            "Bind host (default 127.0.0.1 — set to 0.0.0.0 only with auth or trusted gateway)",
        ),
        ("RTEMPLATE_MCP_PORT", "Bind port (default 40060)"),
        (
            "RUST_LOG",
            "Log filter (e.g. info,rmcp=warn — default: info in server mode, warn in stdio/cli)",
        ),
    ];

    let mut missing = Vec::new();

    println!("==> Checking required environment variables:");
    for &(var, desc) in REQUIRED_VARS {
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
    for &(var, desc) in OPTIONAL_VARS {
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

fn print_help() {
    // TEMPLATE: Update binary name and command descriptions as you add commands.
    eprintln!(
        "cargo xtask — repo automation for rmcp-template

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
  cargo-generate        Smoke-test real cargo-generate output (--no-cargo-check)
  cargo-generate-post   Internal generated-project rewrite command
  generate-docs         Generate volatile docs and metadata from canonical specs
  check-docs            Validate generated docs and metadata are current
  check-stale-claims    Fail on stale hardcoded template claims
  sync-web-source       Copy apps/web into crates/rtemplate-web/assets/source
  check-web-source-sync Validate bundled web source matches apps/web
  update-aurora-web     Refresh Aurora registry components, validate, then sync
  block-env-commits     Prevent staged .env secrets from being committed
  check-coupled-files   Check common companion-file drift in a diff
  check-file-size       Check staged source files against size budgets
  run-ascii-check       Check or fix tracked source/config/docs ASCII hygiene
  check-plugin-stdio-smoke
                        Smoke-test installed plugin stdio binary
  sync-cargo            Copy Cargo.lock into plugin data directories
  check-version-sync    Validate release manifest version-file parity
  check-release-versions [--base REF] [--head REF] [--mode pr|main] [--json]
                        Validate changed release components have fresh versions/tags
  release-plan          Print changed release components and candidate tags
  bump-version          Bump a component: cargo xtask bump-version template patch
  help                  Show this help

TEMPLATE:
  Add commands by extending the match block in xtask/src/main.rs.
  Keep dependencies minimal — xtask should compile in seconds."
    );
}
