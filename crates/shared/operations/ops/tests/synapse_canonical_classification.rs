use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use soma_ops::{AccessClass, CapabilitySupport, OperationSpec, RiskClass};

const EXPECTED_LEGACY_DIGEST: &str =
    "74c9ed1e345a0c67ca7878a25c7aa73a89f36298c1ca6071a5343cf48dd4c4a1";
const EXPECTED_CLASSIFICATION_DIGEST: &str =
    "d7ebb3bfba204301bb3bc3406721f920a3b06cb1af395baff88fcf2d84ea5021";

#[derive(Debug, Deserialize)]
struct ClassificationBundle {
    format_version: u64,
    legacy_semantic_sha256: String,
    classification_sha256: String,
    operation_count: usize,
    operations: Vec<OperationSpec>,
}

#[derive(Debug, Deserialize)]
struct LegacyBundle {
    semantic_sha256: String,
    operations: Vec<LegacyOperation>,
}

#[derive(Debug, Deserialize)]
struct LegacyOperation {
    canonical_name: String,
    legacy_access: String,
    required_params: Vec<String>,
    required_any: Vec<Vec<String>>,
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../docs/unify/03-contracts/examples")
        .join(name)
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
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn count_field(operations: &[Value], field: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for operation in operations {
        let value = operation[field].to_string().trim_matches('"').to_owned();
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

#[test]
fn canonical_classifications_cover_and_validate_all_pinned_operations() {
    let raw_text = std::fs::read_to_string(fixture_path("synapse-canonical-operations.json"))
        .expect("read canonical classification fixture");
    let raw: Value = serde_json::from_str(&raw_text).expect("parse raw classification fixture");
    let bundle: ClassificationBundle =
        serde_json::from_str(&raw_text).expect("deserialize canonical OperationSpec records");
    let legacy: LegacyBundle = serde_json::from_str(
        &std::fs::read_to_string(fixture_path("synapse-operations.json"))
            .expect("read legacy semantic fixture"),
    )
    .expect("parse legacy semantic fixture");

    assert_eq!(bundle.format_version, 1);
    assert_eq!(bundle.operation_count, 59);
    assert_eq!(bundle.operations.len(), 59);
    assert_eq!(bundle.legacy_semantic_sha256, EXPECTED_LEGACY_DIGEST);
    assert_eq!(legacy.semantic_sha256, EXPECTED_LEGACY_DIGEST);
    assert_eq!(bundle.classification_sha256, EXPECTED_CLASSIFICATION_DIGEST);
    assert_eq!(digest(&raw["operations"]), EXPECTED_CLASSIFICATION_DIGEST);

    let legacy_by_name = legacy
        .operations
        .into_iter()
        .map(|operation| (operation.canonical_name.clone(), operation))
        .collect::<BTreeMap<_, _>>();
    let mut names = BTreeSet::new();

    for spec in &bundle.operations {
        spec.validate()
            .unwrap_or_else(|error| panic!("invalid canonical spec {}: {error}", spec.name()));
        assert!(names.insert(spec.name().as_str().to_owned()));

        let donor = legacy_by_name
            .get(spec.name().as_str())
            .unwrap_or_else(|| panic!("missing donor operation for {}", spec.name()));
        let expected_access = if donor.legacy_access == "write" {
            AccessClass::Mutation
        } else {
            AccessClass::Read
        };
        assert_eq!(
            spec.access(),
            expected_access,
            "access drift for {}",
            spec.name()
        );
        assert_eq!(
            spec.required().iter().collect::<Vec<_>>(),
            donor
                .required_params
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            "required parameter drift for {}",
            spec.name()
        );
        let actual_any = spec
            .required_any()
            .iter()
            .map(|group| group.iter().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let mut expected_any = donor
            .required_any
            .iter()
            .map(|group| {
                let mut fields = group.iter().map(String::as_str).collect::<Vec<_>>();
                fields.sort_unstable();
                fields
            })
            .collect::<Vec<_>>();
        expected_any.sort();
        let mut actual_any = actual_any;
        actual_any.sort();
        assert_eq!(
            actual_any,
            expected_any,
            "alternative parameter drift for {}",
            spec.name()
        );
        assert!(
            spec.evidence().next().is_some(),
            "missing evidence for {}",
            spec.name()
        );
        assert_eq!(
            spec.parameter_schema().as_str(),
            format!(
                "schema.operations.{}.parameters.v{}",
                spec.name(),
                spec.schema_version()
            )
        );
        assert_eq!(
            spec.result_schema().as_str(),
            format!(
                "schema.operations.{}.result.v{}",
                spec.name(),
                spec.schema_version()
            )
        );
        assert!(
            spec.diagnostic_codes().next().is_some(),
            "missing diagnostic codes for {}",
            spec.name()
        );
        assert!(
            spec.requirements().next().is_some(),
            "missing requirements for {}",
            spec.name()
        );
    }

    assert_eq!(names, legacy_by_name.keys().cloned().collect());

    let operations = raw["operations"].as_array().expect("operations array");
    assert_eq!(
        count_field(operations, "access"),
        BTreeMap::from([("mutation".into(), 21), ("read".into(), 38)])
    );
    assert_eq!(
        count_field(operations, "risk"),
        BTreeMap::from([
            ("destructive".into(), 6),
            ("disruptive".into(), 7),
            ("privileged".into(), 5),
            ("safe".into(), 41),
        ])
    );
    assert_eq!(
        count_field(operations, "planning"),
        BTreeMap::from([
            ("optional".into(), 3),
            ("required".into(), 18),
            ("unsupported".into(), 38),
        ])
    );
    assert_eq!(
        count_field(operations, "fanout"),
        BTreeMap::from([("required".into(), 1), ("unsupported".into(), 58)])
    );

    let fanout = bundle
        .operations
        .iter()
        .filter(|spec| spec.fanout() != CapabilitySupport::Unsupported)
        .collect::<Vec<_>>();
    assert_eq!(fanout.len(), 1);
    assert_eq!(fanout[0].name().as_str(), "host.exec_many");
    assert_eq!(fanout[0].fanout(), CapabilitySupport::Required);

    let risky = bundle
        .operations
        .iter()
        .filter(|spec| spec.risk() >= RiskClass::Destructive)
        .count();
    assert_eq!(risky, 11);
}
