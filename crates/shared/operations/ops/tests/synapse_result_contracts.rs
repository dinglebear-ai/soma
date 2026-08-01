use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use soma_ops::OperationSpec;

const EXPECTED_CLASSIFICATION_DIGEST: &str =
    "d7ebb3bfba204301bb3bc3406721f920a3b06cb1af395baff88fcf2d84ea5021";
const EXPECTED_RESULT_DIGEST: &str =
    "7addf92410b53205dc7ae2b9b80bb58a4ccfbe947e206bb93a8bb439366c3e3a";

#[derive(Debug, Deserialize)]
struct ClassificationBundle {
    classification_sha256: String,
    operations: Vec<OperationSpec>,
}

#[derive(Debug, Deserialize)]
struct ResultBundle {
    classification_sha256: String,
    result_schema_sha256: String,
    schema_count: usize,
    schemas: Vec<ResultRecord>,
}

#[derive(Debug, Deserialize)]
struct ResultRecord {
    operation_name: String,
    schema_id: String,
    family: String,
    schema: Value,
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../docs/unify/03-contracts/examples")
        .join(name)
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    serde_json::from_str(&std::fs::read_to_string(fixture_path(name)).unwrap()).unwrap()
}

fn canonical_json(value: &Value, output: &mut String) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(&value.to_string()),
        Value::String(value) => output.push_str(&serde_json::to_string(value).unwrap()),
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                canonical_json(value, output);
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key).unwrap());
                output.push(':');
                canonical_json(value, output);
            }
            output.push('}');
        }
    }
}

fn digest(value: &Value) -> String {
    let mut canonical = String::new();
    canonical_json(value, &mut canonical);
    format!("{:x}", Sha256::digest(canonical.as_bytes()))
}

#[test]
fn canonical_result_schemas_cover_all_operations_and_match_schema_ids() {
    let classifications: ClassificationBundle = read("synapse-canonical-operations.json");
    let raw: Value = read("synapse-operation-results.json");
    let bundle: ResultBundle = read("synapse-operation-results.json");

    assert_eq!(
        classifications.classification_sha256,
        EXPECTED_CLASSIFICATION_DIGEST
    );
    assert_eq!(bundle.classification_sha256, EXPECTED_CLASSIFICATION_DIGEST);
    assert_eq!(bundle.result_schema_sha256, EXPECTED_RESULT_DIGEST);
    assert_eq!(digest(&raw["schemas"]), EXPECTED_RESULT_DIGEST);
    assert_eq!(bundle.schema_count, 59);
    assert_eq!(bundle.schemas.len(), 59);

    let specs = classifications
        .operations
        .into_iter()
        .map(|spec| (spec.name().as_str().to_owned(), spec))
        .collect::<BTreeMap<_, _>>();
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut families = BTreeSet::new();
    for record in bundle.schemas {
        let spec = specs
            .get(&record.operation_name)
            .unwrap_or_else(|| panic!("unknown operation {}", record.operation_name));
        assert!(names.insert(record.operation_name.clone()));
        assert!(ids.insert(record.schema_id.clone()));
        families.insert(record.family.clone());
        assert_eq!(record.schema_id, spec.result_schema().as_str());
        assert_eq!(record.schema["$id"], record.schema_id);
        assert_eq!(record.schema["additionalProperties"], false);
        assert!(record.schema["required"].as_array().is_some());
        assert!(record.schema["properties"].as_object().is_some());
    }
    assert_eq!(names, specs.keys().cloned().collect());
    assert_eq!(families.len(), 13);
}
