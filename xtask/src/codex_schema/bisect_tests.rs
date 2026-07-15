use super::*;
use std::path::PathBuf;

const RAW_MCP_ELICITATION_PARAMS: &str = include_str!("testdata/raw_mcp_elicitation_params.json");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask parent")
        .to_path_buf()
}

fn load_committed_defs() -> Map<String, Value> {
    let path = repo_root().join(PROTOCOL_SCHEMA_PATH);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed schema at {}: {e}", path.display()));
    let value: Value = serde_json::from_str(&text).expect("committed schema is valid JSON");
    value["definitions"]
        .as_object()
        .expect("committed schema has definitions")
        .clone()
}

#[test]
fn minimize_finds_the_real_elicitation_culprit_among_decoys() {
    // Start from the real, currently-healthy committed schema, then
    // "un-fix" McpServerElicitationRequestParams back to its raw,
    // panic-triggering shape - reproducing the exact historical bug this
    // tool is meant to automate finding.
    let mut defs = load_committed_defs();
    let raw: Value = serde_json::from_str(RAW_MCP_ELICITATION_PARAMS).unwrap();
    defs.insert("McpServerElicitationRequestParams".to_string(), raw);

    // Sanity precondition: the full schema really does panic now.
    let full = typify_probe::probe(&merge::wrap_definitions(defs.clone()));
    assert!(
        full.reproduces_target(),
        "expected the un-fixed schema to panic; got {}",
        full.summary()
    );

    // Build a suspect universe containing the real culprit plus several
    // arbitrary decoys, so the test actually exercises the binary search
    // (not just a trivial one-element universe).
    let mut universe: BTreeSet<String> = defs
        .keys()
        .filter(|k| k.starts_with("Thread") || k.starts_with("Account"))
        .take(12)
        .cloned()
        .collect();
    universe.insert("McpServerElicitationRequestParams".to_string());
    assert!(
        universe.len() > 1,
        "test setup needs more than one suspect to be meaningful"
    );

    let culprits = minimize(&defs, universe);
    assert_eq!(
        culprits,
        vec!["McpServerElicitationRequestParams".to_string()],
        "bisection should narrow down to exactly the real culprit"
    );
}

#[test]
fn minimize_on_a_single_element_universe_returns_it_without_probing() {
    let defs = load_committed_defs();
    let universe: BTreeSet<String> = ["RequestId".to_string()].into_iter().collect();
    let result = minimize(&defs, universe);
    assert_eq!(result, vec!["RequestId".to_string()]);
}

#[test]
fn suspect_universe_falls_back_to_everything_without_a_baseline() {
    // Point at a definitely-nonexistent baseline path is not directly
    // testable without changing cwd (suspect_universe reads a fixed
    // relative path), so this test instead documents the behavior via the
    // production path: when the committed schema *is* present (the normal
    // case in this repo), diffing an unrelated tiny schema against it
    // yields "everything is new" (i.e. the full candidate set), since none
    // of these definition names exist in the real committed schema.
    let mut defs = Map::new();
    defs.insert("TotallyNewDefinition".to_string(), Value::Bool(true));
    defs.insert("AnotherNewOne".to_string(), Value::Bool(true));
    let universe = suspect_universe(&defs);
    assert_eq!(universe.len(), 2);
}

#[test]
fn split_at_partitions_without_overlap_or_loss() {
    let set: BTreeSet<String> = ["a", "b", "c", "d", "e"]
        .into_iter()
        .map(String::from)
        .collect();
    let (first, second) = split_at(&set, 2);
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 3);
    assert!(first.is_disjoint(&second));
    let mut rejoined: BTreeSet<String> = first.union(&second).cloned().collect();
    assert_eq!(rejoined.len(), 5);
    rejoined.clear();
}
