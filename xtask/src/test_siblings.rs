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
        println!("==> check-test-siblings: all source files have a _tests.rs sibling");
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

fn crate_src_roots() -> Vec<PathBuf> {
    [
        "crates/soma/src",
        "crates/soma-api/src",
        "crates/soma-cli/src",
        "crates/soma-contracts/src",
        "crates/soma-gateway/src",
        "crates/soma-mcp/src",
        "crates/soma-observability/src",
        "crates/soma-runtime/src",
        "crates/soma-service/src",
        "crates/soma-web/src",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(test)]
#[path = "test_siblings_tests.rs"]
mod tests;
