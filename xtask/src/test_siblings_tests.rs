use super::*;

/// Absolute workspace root.
///
/// `cargo test -p xtask` runs with the crate directory as CWD, not the
/// workspace root - only `main.rs` does `set_current_dir`, and that never
/// runs under the test harness. The path lists in this module are all
/// workspace-relative, so anything that touches the filesystem has to anchor
/// them explicitly rather than trusting CWD.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ must have a parent")
        .to_path_buf()
}

#[test]
fn gateway_src_root_is_checked() {
    assert!(
        crate_src_roots()
            .iter()
            .any(|path| path == &PathBuf::from("crates/shared/mcp/gateway/src"))
    );
}

#[test]
fn expected_sibling_uses_source_stem() {
    let source = PathBuf::from("crates/shared/mcp/gateway/src/config.rs");
    assert_eq!(
        expected_test_sibling(&source),
        PathBuf::from("crates/shared/mcp/gateway/src/config_tests.rs")
    );
}

#[test]
fn matching_source_strips_tests_suffix() {
    let tests = PathBuf::from("crates/shared/mcp/gateway/src/config_tests.rs");
    assert_eq!(
        matching_source(&tests),
        PathBuf::from("crates/shared/mcp/gateway/src/config.rs")
    );
}

/// Every workspace member with a `src/` directory must be classified as
/// either checked ([`CHECKED_SRC_ROOTS`]) or deliberately unchecked
/// ([`UNCHECKED_SRC_ROOTS`]).
///
/// This is the actual guard. Without it, `crate_src_roots()` is a bare
/// allowlist: a new crate is unchecked by default and nothing says so, while
/// `check-test-siblings` keeps printing "all source files have a _tests.rs
/// sibling" - a pass that only means it never looked at the new crate. Adding
/// a member now forces a decision, and the reason string is where that
/// decision gets written down.
#[test]
fn every_workspace_member_src_root_is_classified() {
    let manifest = std::fs::read_to_string(workspace_root().join("Cargo.toml"))
        .expect("read workspace Cargo.toml");
    let manifest: toml::Value = toml::from_str(&manifest).expect("parse workspace Cargo.toml");
    let members = manifest["workspace"]["members"]
        .as_array()
        .expect("[workspace] members is an array");
    assert!(
        !members.is_empty(),
        "no workspace members parsed - this test would pass vacuously"
    );

    let checked: Vec<PathBuf> = crate_src_roots();
    let unchecked: Vec<PathBuf> = UNCHECKED_SRC_ROOTS
        .iter()
        .map(|(path, _)| PathBuf::from(path))
        .collect();

    let mut unclassified = Vec::new();
    for member in members {
        let member = member.as_str().expect("workspace member is a string");
        let root = PathBuf::from(member).join("src");
        // A member without a src/ dir (or one that has been removed) has
        // nothing to classify.
        if !workspace_root().join(&root).exists() {
            continue;
        }
        if !checked.contains(&root) && !unchecked.contains(&root) {
            unclassified.push(root.display().to_string());
        }
    }

    assert!(
        unclassified.is_empty(),
        "these workspace members' src roots are in neither CHECKED_SRC_ROOTS nor \
         UNCHECKED_SRC_ROOTS, so check-test-siblings silently ignores them while still \
         reporting success: {unclassified:?}. Add each to CHECKED_SRC_ROOTS, or to \
         UNCHECKED_SRC_ROOTS with the reason it is exempt."
    );
}

/// The two lists must not overlap, and every entry must name a real
/// directory - a typo in either list would otherwise silently drop a tree out
/// of the classification the test above enforces.
#[test]
fn classified_src_roots_are_disjoint_and_real() {
    for (path, reason) in UNCHECKED_SRC_ROOTS {
        assert!(
            !CHECKED_SRC_ROOTS.contains(path),
            "{path} is in both CHECKED_SRC_ROOTS and UNCHECKED_SRC_ROOTS"
        );
        assert!(
            !reason.trim().is_empty(),
            "{path} is exempt without a reason"
        );
        assert!(
            workspace_root().join(path).exists(),
            "UNCHECKED_SRC_ROOTS names {path}, which does not exist"
        );
    }
    for path in CHECKED_SRC_ROOTS {
        assert!(
            workspace_root().join(path).exists(),
            "CHECKED_SRC_ROOTS names {path}, which does not exist"
        );
    }
}
