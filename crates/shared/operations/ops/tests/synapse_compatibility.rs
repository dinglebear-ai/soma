use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use soma_ops::OperationName;

const EXPECTED_DONOR_COMMIT: &str = "8f1bb2efc1a519c9d3b1b5b41ea8bb2ba178011f";
const EXPECTED_SEMANTIC_DIGEST: &str =
    "74c9ed1e345a0c67ca7878a25c7aa73a89f36298c1ca6071a5343cf48dd4c4a1";
const SOURCE_PATH: &str = "src/actions/operations.rs";

#[derive(Debug, Deserialize)]
struct Fixture {
    format_version: u64,
    donor: DonorFixture,
    operation_count: usize,
    semantic_sha256: String,
    operations: Vec<OperationFixture>,
}

#[derive(Debug, Deserialize)]
struct DonorFixture {
    repository: String,
    commit: String,
    source_path: String,
    source_sha256: String,
}

#[derive(Debug, Deserialize)]
struct OperationFixture {
    legacy_name: String,
    canonical_name: String,
    legacy_tool: String,
    legacy_action: String,
    legacy_subaction: Option<String>,
    legacy_access: String,
    legacy_scope: Option<String>,
    legacy_destructive: bool,
    legacy_transport: String,
    required_params: Vec<String>,
    required_any: Vec<Vec<String>>,
    source_path: String,
    source_line: usize,
    source_macro_sha256: String,
}

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../docs/unify/03-contracts/examples/synapse-operations.json")
}

fn load_fixture() -> (Fixture, Value) {
    let text = std::fs::read_to_string(fixture_path()).expect("read pinned Synapse fixture");
    let typed = serde_json::from_str(&text).expect("parse typed pinned Synapse fixture");
    let raw = serde_json::from_str(&text).expect("parse raw pinned Synapse fixture");
    (typed, raw)
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        Value::Object(map) => {
            let sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        scalar => scalar.clone(),
    }
}

fn semantic_digest(value: &Value) -> String {
    let canonical = canonicalize(value);
    let encoded = serde_json::to_vec(&canonical).expect("serialize canonical semantic fixture");
    Sha256::digest(encoded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn assert_unique_non_empty(values: &[String], label: &str) {
    let mut seen = BTreeSet::new();
    for value in values {
        assert!(!value.is_empty(), "{label} contains an empty value");
        assert!(seen.insert(value), "{label} contains duplicate {value}");
    }
}

#[test]
fn pinned_synapse_fixture_preserves_complete_legacy_semantics() {
    let (fixture, raw) = load_fixture();

    assert_eq!(fixture.format_version, 2);
    assert_eq!(fixture.operation_count, 59);
    assert_eq!(fixture.operations.len(), 59);
    assert_eq!(
        fixture.donor.repository,
        "https://github.com/dinglebear-ai/synapse"
    );
    assert_eq!(fixture.donor.commit, EXPECTED_DONOR_COMMIT);
    assert_eq!(fixture.donor.source_path, SOURCE_PATH);
    assert!(is_lower_hex(&fixture.donor.source_sha256, 64));
    assert_eq!(fixture.semantic_sha256, EXPECTED_SEMANTIC_DIGEST);
    assert_eq!(semantic_digest(&raw["operations"]), fixture.semantic_sha256);

    let mut legacy_names = BTreeSet::new();
    let mut canonical_names = BTreeSet::new();
    let mut shapes = BTreeSet::new();
    let mut source_lines = Vec::new();
    let mut tool_counts = BTreeMap::new();
    let mut access_counts = BTreeMap::new();
    let mut transport_counts = BTreeMap::new();
    let mut destructive_count = 0usize;
    let mut alternative_count = 0usize;

    for operation in &fixture.operations {
        assert!(
            legacy_names.insert(operation.legacy_name.as_str()),
            "duplicate legacy operation {}",
            operation.legacy_name
        );
        let parsed = OperationName::new(operation.canonical_name.clone()).unwrap_or_else(|error| {
            panic!("invalid mapping for {}: {error}", operation.legacy_name)
        });
        assert_eq!(parsed.as_str(), operation.canonical_name);
        assert!(
            canonical_names.insert(operation.canonical_name.as_str()),
            "duplicate canonical operation {}",
            operation.canonical_name
        );

        assert!(matches!(
            operation.legacy_tool.as_str(),
            "both" | "flux" | "scout"
        ));
        assert!(!operation.legacy_action.is_empty());
        if let Some(subaction) = &operation.legacy_subaction {
            assert!(!subaction.is_empty());
        }
        assert!(
            shapes.insert((
                operation.legacy_tool.as_str(),
                operation.legacy_action.as_str(),
                operation.legacy_subaction.as_deref(),
            )),
            "duplicate legacy operation shape for {}",
            operation.legacy_name
        );

        let expected_scope = match operation.legacy_access.as_str() {
            "public" => None,
            "read" => Some("synapse:read"),
            "write" => Some("synapse:write"),
            other => panic!("invalid access {other} for {}", operation.legacy_name),
        };
        assert_eq!(operation.legacy_scope.as_deref(), expected_scope);
        if operation.legacy_destructive {
            assert_eq!(operation.legacy_access, "write");
            destructive_count += 1;
        }
        assert!(matches!(
            operation.legacy_transport.as_str(),
            "rest" | "mcp_only"
        ));

        assert_unique_non_empty(
            &operation.required_params,
            &format!("required_params for {}", operation.legacy_name),
        );
        let mut alternatives = BTreeSet::new();
        for group in &operation.required_any {
            assert!(
                !group.is_empty(),
                "empty alternative for {}",
                operation.legacy_name
            );
            assert_unique_non_empty(
                group,
                &format!("required_any for {}", operation.legacy_name),
            );
            assert!(
                alternatives.insert(group),
                "duplicate alternative for {}",
                operation.legacy_name
            );
            alternative_count += 1;
        }

        assert_eq!(operation.source_path, SOURCE_PATH);
        assert!(operation.source_line > 0);
        assert!(is_lower_hex(&operation.source_macro_sha256, 64));
        source_lines.push(operation.source_line);

        *tool_counts
            .entry(operation.legacy_tool.as_str())
            .or_insert(0usize) += 1;
        *access_counts
            .entry(operation.legacy_access.as_str())
            .or_insert(0usize) += 1;
        *transport_counts
            .entry(operation.legacy_transport.as_str())
            .or_insert(0usize) += 1;
    }

    assert!(source_lines.windows(2).all(|window| window[0] < window[1]));
    assert_eq!(
        tool_counts,
        BTreeMap::from([("both", 1), ("flux", 42), ("scout", 16)])
    );
    assert_eq!(
        access_counts,
        BTreeMap::from([("public", 1), ("read", 37), ("write", 21)])
    );
    assert_eq!(
        transport_counts,
        BTreeMap::from([("mcp_only", 45), ("rest", 14)])
    );
    assert_eq!(destructive_count, 12);
    assert_eq!(alternative_count, 2);
}
