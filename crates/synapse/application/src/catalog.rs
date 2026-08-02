use std::{collections::BTreeMap, sync::OnceLock};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use soma_ops::{DiagnosticCode, OperationName, OperationSpec};

use crate::{
    CompatibilityError, DiagnosticProjection, LegacyOperationBinding, LegacyTool,
    binding::LegacyBindingKey,
    schema::{
        CanonicalBundle, DiagnosticBundle, LegacyBundle, OperationSchemaContract, ParameterBundle,
        ResultBundle, build_parameter_schemas, build_result_schemas,
    },
};

const LEGACY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/unify/03-contracts/examples/synapse-operations.json"
));
const CANONICAL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/unify/03-contracts/examples/synapse-canonical-operations.json"
));
const PARAMETERS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/unify/03-contracts/examples/synapse-operation-parameters.json"
));
const RESULTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/unify/03-contracts/examples/synapse-operation-results.json"
));
const DIAGNOSTICS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/unify/03-contracts/examples/operation-diagnostic-projections.json"
));
const EXPECTED_OPERATIONS: usize = 59;
const EXPECTED_DIAGNOSTICS: usize = 33;

/// Embedded canonical Synapse catalog and product compatibility registry.
pub struct SynapseCatalog {
    operations: BTreeMap<OperationName, OperationSpec>,
    bindings: Vec<LegacyOperationBinding>,
    binding_index: BTreeMap<LegacyBindingKey, usize>,
    parameter_schemas: BTreeMap<OperationName, OperationSchemaContract>,
    result_schemas: BTreeMap<OperationName, OperationSchemaContract>,
    diagnostics: BTreeMap<DiagnosticCode, DiagnosticProjection>,
}

impl SynapseCatalog {
    /// Returns the process-wide checked-in catalog.
    #[must_use]
    pub fn embedded() -> &'static Self {
        static CATALOG: OnceLock<SynapseCatalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            Self::try_from_embedded().expect("checked-in Synapse contracts must remain valid")
        })
    }

    /// Parses and cross-validates every checked-in compatibility artifact.
    pub fn try_from_embedded() -> Result<Self, CompatibilityError> {
        let canonical: CanonicalBundle = parse("synapse-canonical-operations.json", CANONICAL)?;
        let legacy: LegacyBundle = parse("synapse-operations.json", LEGACY)?;
        let parameters: ParameterBundle = parse("synapse-operation-parameters.json", PARAMETERS)?;
        let results: ResultBundle = parse("synapse-operation-results.json", RESULTS)?;
        let diagnostics: DiagnosticBundle =
            parse("operation-diagnostic-projections.json", DIAGNOSTICS)?;

        if canonical.operations.len() != EXPECTED_OPERATIONS
            || legacy.operations.len() != EXPECTED_OPERATIONS
            || parameters.schemas.len() != EXPECTED_OPERATIONS
            || results.schemas.len() != EXPECTED_OPERATIONS
        {
            return contract_error("catalog", "expected 59 records in every operation artifact");
        }
        if diagnostics.mappings.len() != EXPECTED_DIAGNOSTICS {
            return contract_error("catalog", "expected 33 diagnostic projections");
        }
        for digest in [
            &parameters.classification_sha256,
            &results.classification_sha256,
            &diagnostics.classification_sha256,
        ] {
            if digest != &canonical.classification_sha256 {
                return contract_error("catalog", "classification digest mismatch");
            }
        }

        let mut operations = BTreeMap::new();
        for operation in canonical.operations {
            operation
                .validate()
                .map_err(|error| CompatibilityError::EmbeddedContract {
                    artifact: "synapse-canonical-operations.json",
                    message: format!("{}: {error}", operation.name()),
                })?;
            if operations
                .insert(operation.name().clone(), operation)
                .is_some()
            {
                return contract_error("catalog", "duplicate canonical operation");
            }
        }

        let mut bindings = Vec::with_capacity(EXPECTED_OPERATIONS);
        let mut binding_index = BTreeMap::new();
        for binding in legacy.operations {
            let operation = operations.get(binding.canonical_name()).ok_or_else(|| {
                CompatibilityError::EmbeddedContract {
                    artifact: "synapse-operations.json",
                    message: format!(
                        "binding {} targets missing {}",
                        binding.legacy_name(),
                        binding.canonical_name()
                    ),
                }
            })?;
            validate_binding_parity(&binding, operation)?;
            let key = LegacyBindingKey::new(binding.tool(), binding.action(), binding.subaction());
            let index = bindings.len();
            if binding_index.insert(key, index).is_some() {
                return contract_error("synapse-operations.json", "duplicate legacy routing key");
            }
            bindings.push(binding);
        }

        let parameter_schemas = build_parameter_schemas(parameters, &operations)?;
        let result_schemas = build_result_schemas(results, &operations)?;
        let mut projected = BTreeMap::new();
        for projection in diagnostics.mappings {
            if projected
                .insert(projection.code().clone(), projection)
                .is_some()
            {
                return contract_error(
                    "operation-diagnostic-projections.json",
                    "duplicate diagnostic code",
                );
            }
        }
        for operation in operations.values() {
            for code in operation.diagnostic_codes() {
                if !projected.contains_key(code) {
                    return contract_error(
                        "operation-diagnostic-projections.json",
                        &format!("missing projection for {code}"),
                    );
                }
            }
        }

        Ok(Self {
            operations,
            bindings,
            binding_index,
            parameter_schemas,
            result_schemas,
            diagnostics: projected,
        })
    }

    /// Returns the number of canonical operations.
    #[must_use]
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Returns the number of legacy bindings.
    #[must_use]
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Returns the number of stable diagnostic projections.
    #[must_use]
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.len()
    }

    /// Iterates over canonical operation specifications.
    pub fn operations(&self) -> impl Iterator<Item = &OperationSpec> {
        self.operations.values()
    }

    /// Iterates over all product-owned legacy bindings.
    pub fn bindings(&self) -> impl Iterator<Item = &LegacyOperationBinding> {
        self.bindings.iter()
    }

    /// Returns a canonical operation specification.
    #[must_use]
    pub fn operation(&self, name: &OperationName) -> Option<&OperationSpec> {
        self.operations.get(name)
    }

    /// Resolves a Flux or Scout route to its product-owned binding.
    #[must_use]
    pub fn binding(
        &self,
        tool: LegacyTool,
        action: &str,
        subaction: Option<&str>,
    ) -> Option<&LegacyOperationBinding> {
        let direct = LegacyBindingKey::new(tool, action, subaction);
        let shared = LegacyBindingKey::new(LegacyTool::Both, action, subaction);
        self.binding_index
            .get(&direct)
            .or_else(|| self.binding_index.get(&shared))
            .map(|index| &self.bindings[*index])
    }

    /// Returns a canonical parameter schema.
    #[must_use]
    pub fn parameter_schema(&self, operation: &OperationName) -> Option<&OperationSchemaContract> {
        self.parameter_schemas.get(operation)
    }

    /// Returns a canonical result schema.
    #[must_use]
    pub fn result_schema(&self, operation: &OperationName) -> Option<&OperationSchemaContract> {
        self.result_schemas.get(operation)
    }

    /// Returns a global diagnostic surface projection.
    #[must_use]
    pub fn diagnostic_projection(&self, code: &DiagnosticCode) -> Option<&DiagnosticProjection> {
        self.diagnostics.get(code)
    }

    /// Returns bindings available through one legacy MCP tool.
    pub fn bindings_for_tool(
        &self,
        tool: LegacyTool,
    ) -> impl Iterator<Item = &LegacyOperationBinding> {
        self.bindings
            .iter()
            .filter(move |binding| binding.tool() == tool || binding.tool() == LegacyTool::Both)
    }

    /// Generates the closed legacy MCP input schema for Flux or Scout.
    #[must_use]
    pub fn legacy_tool_schema(&self, tool: LegacyTool) -> Value {
        let branches = self
            .bindings_for_tool(tool)
            .filter_map(|binding| {
                let contract = self.parameter_schemas.get(binding.canonical_name())?;
                let mut branch = contract.schema().clone();
                let object = branch.as_object_mut()?;
                object.remove("$schema");
                object.remove("$id");
                object.remove("title");
                let properties = object
                    .entry("properties")
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()?;
                properties.insert("action".into(), json!({"const": binding.action()}));
                if let Some(subaction) = binding.subaction() {
                    properties.insert("subaction".into(), json!({"const": subaction}));
                }
                let presentation = json!({"type":"string","enum":["markdown","json"]});
                properties.insert("response_format".into(), presentation.clone());
                properties.insert("format".into(), presentation);
                let required = object
                    .entry("required")
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()?;
                required.push(Value::String("action".into()));
                if binding.subaction().is_some() {
                    required.push(Value::String("subaction".into()));
                }
                Some(branch)
            })
            .collect::<Vec<_>>();
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "oneOf": branches
        })
    }
}

fn validate_binding_parity(
    binding: &LegacyOperationBinding,
    operation: &OperationSpec,
) -> Result<(), CompatibilityError> {
    let required = operation.required().iter().collect::<Vec<_>>();
    let mut legacy_required = binding
        .required_params()
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    legacy_required.sort_unstable();
    if required != legacy_required {
        return contract_error(
            "synapse-operations.json",
            &format!("required field drift for {}", operation.name()),
        );
    }
    let mut canonical_any = operation
        .required_any()
        .iter()
        .map(|group| group.iter().map(str::to_owned).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    canonical_any.sort();
    let mut legacy_any = binding.required_any().to_vec();
    for group in &mut legacy_any {
        group.sort();
    }
    legacy_any.sort();
    if canonical_any != legacy_any {
        return contract_error(
            "synapse-operations.json",
            &format!("alternative field drift for {}", operation.name()),
        );
    }
    Ok(())
}

fn parse<T: DeserializeOwned>(
    artifact: &'static str,
    input: &str,
) -> Result<T, CompatibilityError> {
    serde_json::from_str(input).map_err(|error| CompatibilityError::EmbeddedContract {
        artifact,
        message: error.to_string(),
    })
}

pub(crate) fn contract_error<T>(
    artifact: &'static str,
    message: &str,
) -> Result<T, CompatibilityError> {
    Err(CompatibilityError::EmbeddedContract {
        artifact,
        message: message.to_owned(),
    })
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
