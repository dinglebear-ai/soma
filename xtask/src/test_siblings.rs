use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const EXEMPT: &[&str] = &["main.rs", "lib.rs"];
const ORPHAN_EXEMPT: &[&str] = &["cli_tests.rs", "mcp_tests.rs"];

pub(crate) fn check() -> Result<()> {
    let missing = missing_siblings();
    let orphans = orphaned_test_files();
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
        // Name the scope, not just the verdict. This used to print "all source
        // files have a _tests.rs sibling" while silently skipping every tree
        // missing from the roots list - a claim much broader than what it had
        // actually looked at. Printing the skipped trees and why keeps the
        // pass honest, and puts the exemptions in front of whoever runs the
        // command rather than only in the source.
        println!(
            "==> check-test-siblings: all source files have a _tests.rs sibling ({} tree(s) \
             checked)",
            CHECKED_SRC_ROOTS.len()
        );
        println!(
            "    not checked ({} tree(s), by design - see UNCHECKED_SRC_ROOTS):",
            UNCHECKED_SRC_ROOTS.len()
        );
        for (path, reason) in UNCHECKED_SRC_ROOTS {
            println!("      {path}: {reason}");
        }
        return Ok(());
    }
    bail!("{} missing, {} orphaned", missing.len(), orphans.len());
}

fn missing_siblings() -> Vec<PathBuf> {
    crate_src_roots()
        .into_iter()
        .flat_map(source_files_requiring_siblings)
        .filter(|path| !expected_test_sibling(path).exists())
        .collect()
}

fn orphaned_test_files() -> Vec<PathBuf> {
    crate_src_roots()
        .into_iter()
        .flat_map(test_files)
        .filter(|path| !matching_source(path).exists())
        .collect()
}

fn source_files_requiring_siblings(root: PathBuf) -> Vec<PathBuf> {
    rust_files(root)
        .into_iter()
        .filter(|path| {
            let name = filename(path);
            !name.ends_with("_tests.rs") && !EXEMPT.contains(&name.as_str())
        })
        .collect()
}

fn test_files(root: PathBuf) -> Vec<PathBuf> {
    rust_files(root)
        .into_iter()
        .filter(|path| {
            let name = filename(path);
            name.ends_with("_tests.rs") && !ORPHAN_EXEMPT.contains(&name.as_str())
        })
        .collect()
}

fn rust_files(root: PathBuf) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| filename(path).ends_with(".rs"))
        .collect()
}

fn expected_test_sibling(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap().to_string_lossy();
    path.parent().unwrap().join(format!("{stem}_tests.rs"))
}

fn matching_source(path: &Path) -> PathBuf {
    let stem = filename(path).trim_end_matches("_tests.rs").to_owned();
    path.parent().unwrap().join(format!("{stem}.rs"))
}

fn filename(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// Source trees that follow the `foo.rs` + `foo_tests.rs` sibling convention
/// and are therefore checked by this command.
///
/// Every workspace member's `src/` must appear either here or in
/// [`UNCHECKED_SRC_ROOTS`]; `every_workspace_member_src_root_is_classified`
/// fails the build otherwise. That is the point of splitting the two lists:
/// this used to be a bare allowlist, so a crate that was simply never added
/// to it was silently unchecked, and the command still reported "all source
/// files have a _tests.rs sibling" - a pass that only meant it had not
/// looked.
const CHECKED_SRC_ROOTS: &[&str] = &[
    "apps/soma/src",
    "crates/shared/codemode/src",
    "crates/shared/mcp/client/src",
    "crates/shared/mcp/gateway/src",
    "crates/shared/mcp/proxy/src",
    "crates/shared/mcp/server/src",
    "crates/shared/observability/src",
    "crates/shared/openapi/src",
    "crates/soma/api/src",
    "crates/soma/cli/src",
    "crates/soma/client/src",
    "crates/soma/config/src",
    "crates/soma/integrations/src",
    "crates/soma/mcp/src",
    "crates/soma/palette/src",
    "crates/soma/web/src",
];

/// Source trees deliberately *not* checked, each with the reason.
///
/// Three test layouts genuinely coexist in this repo. The sibling convention
/// is the default for Soma's own crates; some trees use inline
/// `#[cfg(test)] mod tests`; and at least one tests entirely through its
/// public API from `tests/`. Forcing siblings on the latter two would be
/// mechanical churn that buys nothing, or would defeat the point outright. An
/// entry here is a decision, not an oversight - which is exactly what the old
/// bare allowlist could not express.
const UNCHECKED_SRC_ROOTS: &[(&str, &str)] = &[
    (
        "crates/integrations/unifi/src",
        "newly vendored (crates/integrations/ layer) with inline #[cfg(test)] \
         mod tests throughout - no siblings anywhere in the crate yet.",
    ),
    (
        "crates/shared/auth/src",
        "inline #[cfg(test)] mod tests throughout (21 modules, 0 siblings)",
    ),
    (
        "crates/shared/cli-core/src",
        "extracted from soma-cli with its tests still inline - predates the \
         sibling convention. Tracked separately; move this entry to \
         CHECKED_SRC_ROOTS once it gets siblings.",
    ),
    (
        "crates/shared/codex-app-server-client/src",
        "inline #[cfg(test)] mod tests throughout. This crate is designed to be \
         lifted wholesale into another repo (see its README.md), so its tests \
         travel inside the files they cover rather than depending on this \
         repo's sibling layout.",
    ),
    (
        "crates/shared/http-api/src",
        "extracted from soma-api with its tests still inline - predates the \
         sibling convention. Tracked separately; move this entry to \
         CHECKED_SRC_ROOTS once it gets siblings.",
    ),
    (
        "crates/shared/http-server/src",
        "extracted from apps/soma with its tests still inline - predates the \
         sibling convention. Tracked separately; move this entry to \
         CHECKED_SRC_ROOTS once it gets siblings.",
    ),
    (
        "crates/shared/provider-adapters/src",
        "follows the sibling convention but does not satisfy it yet - error.rs \
         has no sibling. Tracked separately; move this entry to \
         CHECKED_SRC_ROOTS once it does.",
    ),
    (
        "crates/shared/provider-core/src",
        "tests exclusively through the public API from tests/ (22 tests across \
         7 files) - no inline modules and no siblings anywhere in the crate. \
         That is the point: the crate is the provider contract, so its tests \
         exercise it the way a provider author would rather than reaching into \
         private internals. Siblings here would invite the opposite.",
    ),
    (
        "crates/shared/tauri-shell/src",
        "extracted with window.rs/app.rs/tray.rs still untested (Tauri desktop \
         windowing needs a display to exercise) - no siblings for those three \
         yet. Tracked separately; move this entry to CHECKED_SRC_ROOTS once \
         they do.",
    ),
    (
        "crates/shared/traces/src",
        "inline #[cfg(test)] mod tests throughout",
    ),
    (
        "crates/soma/application/src",
        "follows the sibling convention but does not satisfy it yet - types.rs, \
         context.rs, ports.rs and error.rs have no sibling. Tracked \
         separately; move this entry to CHECKED_SRC_ROOTS once they do.",
    ),
    (
        "crates/soma/domain/src",
        "follows the sibling convention but does not satisfy it yet - \
         execution.rs and principal.rs have no sibling. Tracked separately; \
         move this entry to CHECKED_SRC_ROOTS once they do.",
    ),
    (
        "crates/soma/runtime/src",
        "follows the sibling convention except test_support.rs, a \
         `#![cfg(test)]` dev-dependency-only helper module - it IS test \
         infrastructure, not source under test, so it has no _tests.rs \
         sibling by design.",
    ),
    (
        "crates/soma/test-support/src",
        "test-support code is exercised by the crates that consume it",
    ),
    (
        "xtask/src",
        "mixed by module: xtask/src/codex_schema/ uses siblings, most other \
         modules use inline tests. Split per-module rather than per-crate before \
         checking this tree.",
    ),
    (
        "crates/integrations/unifi/src",
        "inline #[cfg(test)] mod tests throughout (63 as of writing), plus \
         tests/client.rs and tests/action_dispatch.rs exercising the HTTP \
         and dynamic-dispatch layers through the public API. This is the \
         crates/integrations/* reference template's convention - see \
         crates/integrations/README.md.",
    ),
];

fn crate_src_roots() -> Vec<PathBuf> {
    CHECKED_SRC_ROOTS.iter().map(PathBuf::from).collect()
}

#[cfg(test)]
#[path = "test_siblings_tests.rs"]
mod tests;
