//! `cargo xtask check-ts-client [--write|--check]` - keeps
//! `crates/shared/codex-app-server-client/clients/typescript/src/generated/openapi-types.ts`
//! (a checked-in, `openapi-typescript`-generated TypeScript client for that
//! crate's `rest` feature - see that directory's own README.md) in sync with
//! the crate's checked-in `openapi.json`, and (in `--check` mode) verifies
//! the package still type-checks.
//!
//! This is the TypeScript-side analogue of [`super::scripts_lane_d::check_openapi`]
//! (which keeps `docs/generated/openapi.json` in sync with the main `soma`
//! binary's own OpenAPI surface) - same `--write` regenerates /
//! `--check` verifies shape, different source of truth and a different
//! generator (`openapi-typescript`'s JS API, not a Rust builder function).
//!
//! # Why this shells out to `pnpm`, not a Rust HTTP-schema crate
//!
//! The TypeScript client's whole point (see bead `rmcp-template-g0qf.5`) is
//! proving `openapi.json` is consumable by a real, independent TypeScript
//! toolchain - reimplementing `openapi-typescript`'s logic in Rust here would
//! defeat that. `xtask` itself has no Node/pnpm dependency; this module only
//! *drives* the package's own `package.json` scripts
//! (`clients/typescript/scripts/generate.mjs` and `.../check-sync` /
//! `.../typecheck`), the same way a human contributor would.
//!
//! # Why a graceful skip, not a hard failure, when `node`/`pnpm` are missing
//!
//! Mirrors `codex_schema::drift`'s posture on a missing `codex` CLI:
//! this repo's self-hosted CI runners are not guaranteed to have a
//! Node/pnpm toolchain provisioned (see docs/CI.md), and a drift check that
//! hard-fails whenever the runner's toolchain lags reads as CI flakiness, not
//! a real problem with this PR's diff. A skip is loud (printed, not
//! swallowed) and still exits `0`, so the check is "yes if we can tell,
//! silent-not-lying if we can't" rather than a false failure.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::scripts_lane_d::CheckMode;

const TS_CLIENT_DIR: &str = "crates/shared/codex-app-server-client/clients/typescript";

const USAGE: &str = "Usage: cargo xtask check-ts-client [--write] [--check]

  --write  Regenerate src/generated/openapi-types.ts from ../../openapi.json
           (via `pnpm run generate`) and overwrite the checked-in file.
  --check  (default) Verify src/generated/openapi-types.ts is byte-identical
           to what `pnpm run generate` would produce, then run `pnpm run
           typecheck` (`tsc --noEmit`) and `pnpm test` over the whole package.

Requires `node` and `pnpm` on PATH. If either is missing, this prints a skip
message and exits 0 rather than failing - see xtask/src/ts_client.rs's module
docs for why.";

pub fn run(args: &[String]) -> Result<()> {
    // Shares `CheckMode` with `check-openapi`/`check-schema-docs` rather than
    // hand-rolling a third parser for the same two flags: they are the same
    // grammar for the same job (regenerate-or-verify a checked-in generated
    // artifact), and a divergence between them would be an accident rather
    // than a decision.
    let Some(mode) = CheckMode::parse(args, USAGE)? else {
        return Ok(());
    };
    let root = current_dir()?;
    let package_dir = root.join(TS_CLIENT_DIR);
    if !package_dir.join("package.json").is_file() {
        bail!(
            "{} is missing package.json - expected the checked-in TypeScript REST client \
             package (see crates/shared/codex-app-server-client/clients/typescript/README.md)",
            package_dir.display()
        );
    }

    if let Some(reason) = missing_toolchain_reason() {
        println!(
            "check-ts-client: skipped: {reason}. Cannot verify \
             {TS_CLIENT_DIR}/src/generated/openapi-types.ts is in sync with openapi.json. \
             Install Node.js and pnpm (e.g. via mise) to run this check locally or in CI."
        );
        return Ok(());
    }

    run_pnpm(&package_dir, &["install", "--frozen-lockfile"]).context(
        "`pnpm install --frozen-lockfile` failed for the TypeScript REST client package",
    )?;

    // Ordered write-then-check so `--write --check` (`CheckMode::CheckAndWrite`)
    // regenerates and then verifies the result, matching what the same flag
    // pair does for `check-openapi`.
    if mode.should_write() {
        run_pnpm(&package_dir, &["run", "generate"]).context("`pnpm run generate` failed")?;
        println!("check-ts-client: wrote {TS_CLIENT_DIR}/src/generated/openapi-types.ts");
    }
    if mode.should_check() {
        run_pnpm(&package_dir, &["run", "check-sync"]).context(
            "TypeScript REST client types are out of sync with openapi.json - regenerate \
             with `cargo xtask check-ts-client --write` and commit the diff",
        )?;
        run_pnpm(&package_dir, &["run", "typecheck"])
            .context("TypeScript REST client failed to typecheck (`pnpm run typecheck`)")?;
        // The client hand-rolls SSE wire-format parsing and path-segment
        // encoding, neither of which `tsc` can say anything about. Both had
        // real bugs that only a test could catch - a stream that leaked its
        // connection on early exit, and `..` segments that silently retargeted
        // a request to a different route. The suite is zero-dependency
        // (`node:test`) and runs in well under a second, so there's no reason
        // for the gate to stop at typecheck.
        run_pnpm(&package_dir, &["test"])
            .context("TypeScript REST client test suite failed (`pnpm test`)")?;
        println!(
            "check-ts-client: {TS_CLIENT_DIR} is in sync with openapi.json, type-checks, and \
             passes its tests"
        );
    }
    Ok(())
}

/// `None` when both `node` and `pnpm` are usable; otherwise a human-readable
/// reason naming the first missing tool.
fn missing_toolchain_reason() -> Option<String> {
    for bin in ["node", "pnpm"] {
        if !crate::command_exists(bin) {
            return Some(format!("`{bin}` not found on PATH"));
        }
    }
    None
}

fn run_pnpm(package_dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("pnpm")
        .args(args)
        .current_dir(package_dir)
        .status()
        .with_context(|| format!("failed to run `pnpm {}`", args.join(" ")))?;
    if !status.success() {
        bail!("`pnpm {}` exited with status {status}", args.join(" "));
    }
    Ok(())
}

fn current_dir() -> Result<PathBuf> {
    std::env::current_dir().context("failed to read current directory")
}

#[cfg(test)]
mod tests {
    use super::*;

    // These pin the flag grammar *as this command consumes it*. The parser
    // itself is `CheckMode`, shared with `check-openapi`/`check-schema-docs`
    // and unit-tested there; what's worth pinning here is that this command
    // agrees with the others - most importantly that `--write --check` means
    // "regenerate, then verify" rather than being rejected, which is what it
    // used to do back when this file hand-rolled its own parser.
    fn parse(args: &[&str]) -> Result<Option<CheckMode>> {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        CheckMode::parse(&owned, USAGE)
    }

    #[test]
    fn defaults_to_check() {
        let mode = parse(&[]).unwrap().expect("no --help was passed");
        assert!(mode.should_check());
        assert!(!mode.should_write());
    }

    #[test]
    fn accepts_write_flag() {
        let mode = parse(&["--write"]).unwrap().expect("no --help was passed");
        assert!(mode.should_write());
        assert!(!mode.should_check());
    }

    #[test]
    fn write_and_check_together_regenerate_then_verify() {
        let mode = parse(&["--write", "--check"])
            .unwrap()
            .expect("no --help was passed");
        assert!(mode.should_write());
        assert!(mode.should_check());
    }

    #[test]
    fn help_short_circuits_without_running_the_command() {
        assert_eq!(parse(&["--help"]).unwrap(), None);
        assert_eq!(parse(&["-h"]).unwrap(), None);
    }

    #[test]
    fn rejects_unknown_flag() {
        let error = parse(&["--bogus"]).unwrap_err();
        assert!(error.to_string().contains("unknown option"));
    }

    #[test]
    fn ts_client_dir_points_at_the_checked_in_package() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask/Cargo.toml must have a parent directory");
        assert!(
            root.join(TS_CLIENT_DIR).join("package.json").is_file(),
            "{TS_CLIENT_DIR}/package.json must exist"
        );
        assert!(
            root.join(TS_CLIENT_DIR)
                .join("src/generated/openapi-types.ts")
                .is_file(),
            "{TS_CLIENT_DIR}/src/generated/openapi-types.ts must exist (checked in)"
        );
    }
}
