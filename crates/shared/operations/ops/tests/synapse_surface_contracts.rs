use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use soma_ops::{DiagnosticCode, OperationSpec};

const EXPECTED_CLASSIFICATION_DIGEST: &str =
    "d7ebb3bfba204301bb3bc3406721f920a3b06cb1af395baff88fcf2d84ea5021";
const EXPECTED_PARAMETER_DIGEST: &str =
    "62ff07d6fed6bf18d65fae54708c28ceb3397a4c4cf9f9810a6ef61a68f308fa";
const EXPECTED_PROJECTION_DIGEST: &str =
    "87d9be437bf7a0ac8a21ac32e03e852a98d6ba30591c2ad6009e570cd35fafaa";
const SURFACE_FIELDS: [&str; 4] = ["action", "format", "response_format", "subaction"];

#[derive(Debug, Deserialize)]
struct ClassificationBundle {
    classification_sha256: String,
    operations: Vec<OperationSpec>,
}

#[derive(Debug, Deserialize)]
struct ParameterBundle {
    classification_sha256: String,
    excluded_surface_fields: Vec<String>,
    parameter_schema_sha256: String,
    schema_count: usize,
    schemas: Vec<ParameterRecord>,
}

#[derive(Debug, Deserialize)]
struct ParameterRecord {
    operation_name: String,
    schema_id: String,
    schema: Value,
}

#[derive(Debug, Deserialize)]
struct ProjectionBundle {
    classification_sha256: String,
    projection_sha256: String,
    mapping_count: usize,
    mappings: Vec<Projection>,
}

#[derive(Debug, Deserialize)]
struct Projection {
    code: DiagnosticCode,
    category: String,
    cli_exit_code: u8,
    http_status: u16,
    mcp_error_code: i32,
    event_severity: String,
    retry: String,
    terminal: bool,
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../docs/unify/03-contracts/examples")
        .join(name)
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    serde_json::from_str(
        &std::fs::read_to_string(fixture_path(name))
            .unwrap_or_else(|error| panic!("read {name}: {error}")),
    )
    .unwrap_or_else(|error| panic!("parse {name}: {error}"))
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
fn parameter_schemas_are_closed_complete_and_bound_to_operation_specs() {
    let classifications: ClassificationBundle = read("synapse-canonical-operations.json");
    let raw: Value = read("synapse-operation-parameters.json");
    let bundle: ParameterBundle = read("synapse-operation-parameters.json");

    assert_eq!(
        classifications.classification_sha256,
        EXPECTED_CLASSIFICATION_DIGEST
    );
    assert_eq!(bundle.classification_sha256, EXPECTED_CLASSIFICATION_DIGEST);
    assert_eq!(bundle.parameter_schema_sha256, EXPECTED_PARAMETER_DIGEST);
    assert_eq!(digest(&raw["schemas"]), EXPECTED_PARAMETER_DIGEST);
    assert_eq!(bundle.schema_count, 59);
    assert_eq!(bundle.schemas.len(), 59);
    assert_eq!(
        bundle.excluded_surface_fields,
        SURFACE_FIELDS.map(str::to_owned)
    );

    let specs = classifications
        .operations
        .into_iter()
        .map(|spec| (spec.name().as_str().to_owned(), spec))
        .collect::<BTreeMap<_, _>>();
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();

    for record in bundle.schemas {
        let spec = specs
            .get(&record.operation_name)
            .unwrap_or_else(|| panic!("unknown operation {}", record.operation_name));
        assert!(names.insert(record.operation_name.clone()));
        assert!(ids.insert(record.schema_id.clone()));
        assert_eq!(record.schema_id, spec.parameter_schema().as_str());
        assert_eq!(record.schema["$id"], record.schema_id);
        assert_eq!(record.schema["additionalProperties"], false);

        let properties = record.schema["properties"]
            .as_object()
            .expect("parameter properties");
        for field in SURFACE_FIELDS {
            assert!(
                !properties.contains_key(field),
                "surface field {field} leaked into {}",
                spec.name()
            );
        }
        let required = record.schema["required"]
            .as_array()
            .expect("required array")
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(required, spec.required().iter().collect());
    }

    assert_eq!(names, specs.keys().cloned().collect());
}

#[test]
fn diagnostic_projections_exactly_cover_declared_operation_codes() {
    let classifications: ClassificationBundle = read("synapse-canonical-operations.json");
    let raw: Value = read("operation-diagnostic-projections.json");
    let bundle: ProjectionBundle = read("operation-diagnostic-projections.json");

    assert_eq!(bundle.classification_sha256, EXPECTED_CLASSIFICATION_DIGEST);
    assert_eq!(bundle.projection_sha256, EXPECTED_PROJECTION_DIGEST);
    assert_eq!(digest(&raw["mappings"]), EXPECTED_PROJECTION_DIGEST);
    assert_eq!(bundle.mapping_count, 33);
    assert_eq!(bundle.mappings.len(), 33);

    let declared = classifications
        .operations
        .iter()
        .flat_map(OperationSpec::diagnostic_codes)
        .map(DiagnosticCode::as_str)
        .collect::<BTreeSet<_>>();
    let projected = bundle
        .mappings
        .iter()
        .map(|mapping| mapping.code.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(declared, projected);

    for mapping in &bundle.mappings {
        assert!(!mapping.category.is_empty());
        assert!(mapping.cli_exit_code <= 125);
        assert!((100..=599).contains(&mapping.http_status));
        assert!((-32800..=0).contains(&mapping.mcp_error_code));
        assert!(matches!(
            mapping.event_severity.as_str(),
            "info" | "warning" | "error"
        ));
        assert!(matches!(
            mapping.retry.as_str(),
            "never" | "safe" | "conditional"
        ));
        if !mapping.terminal {
            assert_eq!(mapping.cli_exit_code, 0);
            assert_eq!(mapping.mcp_error_code, 0);
        }
    }
}
