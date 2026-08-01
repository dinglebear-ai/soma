use std::collections::BTreeMap;

use jsonschema::Validator;
use serde::Deserialize;
use serde_json::Value;
use soma_ops::{OperationName, OperationSpec, SchemaId};

use crate::{
    CompatibilityError, DiagnosticProjection, LegacyOperationBinding, catalog::contract_error,
};

/// One compiled parameter or result schema bound to a canonical operation.
pub struct OperationSchemaContract {
    schema_id: SchemaId,
    family: Option<String>,
    schema: Value,
    validator: Validator,
}

impl OperationSchemaContract {
    pub(crate) fn new(
        artifact: &'static str,
        schema_id: SchemaId,
        family: Option<String>,
        schema: Value,
    ) -> Result<Self, CompatibilityError> {
        let validator = jsonschema::validator_for(&schema).map_err(|error| {
            CompatibilityError::EmbeddedContract {
                artifact,
                message: format!("schema {schema_id} failed to compile: {error}"),
            }
        })?;
        Ok(Self {
            schema_id,
            family,
            schema,
            validator,
        })
    }

    /// Returns the stable schema identity.
    #[must_use]
    pub fn schema_id(&self) -> &SchemaId {
        &self.schema_id
    }

    /// Returns the normalized result family, when this is a result contract.
    #[must_use]
    pub fn family(&self) -> Option<&str> {
        self.family.as_deref()
    }

    /// Returns the closed Draft 2020-12 JSON Schema.
    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    pub(crate) fn validate(
        &self,
        operation: &OperationName,
        kind: &'static str,
        value: &Value,
    ) -> Result<(), CompatibilityError> {
        let details = self
            .validator
            .iter_errors(value)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect::<Vec<_>>();
        if details.is_empty() {
            Ok(())
        } else {
            Err(CompatibilityError::SchemaValidation {
                operation: operation.clone(),
                kind,
                details: details.join("; "),
            })
        }
    }
}

pub(crate) fn build_parameter_schemas(
    bundle: ParameterBundle,
    operations: &BTreeMap<OperationName, OperationSpec>,
) -> Result<BTreeMap<OperationName, OperationSchemaContract>, CompatibilityError> {
    let mut schemas = BTreeMap::new();
    for record in bundle.schemas {
        let operation = OperationName::new(record.operation_name.clone()).map_err(|error| {
            CompatibilityError::EmbeddedContract {
                artifact: "synapse-operation-parameters.json",
                message: format!("{}: {error}", record.operation_name),
            }
        })?;
        let spec = operations
            .get(&operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?;
        if &record.schema_id != spec.parameter_schema() {
            return contract_error(
                "synapse-operation-parameters.json",
                &format!("schema identity drift for {operation}"),
            );
        }
        let contract = OperationSchemaContract::new(
            "synapse-operation-parameters.json",
            record.schema_id,
            None,
            record.schema,
        )?;
        if schemas.insert(operation, contract).is_some() {
            return contract_error(
                "synapse-operation-parameters.json",
                "duplicate operation schema",
            );
        }
    }
    if schemas.len() != operations.len() {
        return contract_error(
            "synapse-operation-parameters.json",
            "parameter schema coverage mismatch",
        );
    }
    Ok(schemas)
}

pub(crate) fn build_result_schemas(
    bundle: ResultBundle,
    operations: &BTreeMap<OperationName, OperationSpec>,
) -> Result<BTreeMap<OperationName, OperationSchemaContract>, CompatibilityError> {
    let mut schemas = BTreeMap::new();
    for record in bundle.schemas {
        let operation = OperationName::new(record.operation_name.clone()).map_err(|error| {
            CompatibilityError::EmbeddedContract {
                artifact: "synapse-operation-results.json",
                message: format!("{}: {error}", record.operation_name),
            }
        })?;
        let spec = operations
            .get(&operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?;
        if &record.schema_id != spec.result_schema() {
            return contract_error(
                "synapse-operation-results.json",
                &format!("schema identity drift for {operation}"),
            );
        }
        let contract = OperationSchemaContract::new(
            "synapse-operation-results.json",
            record.schema_id,
            Some(record.family),
            record.schema,
        )?;
        if schemas.insert(operation, contract).is_some() {
            return contract_error(
                "synapse-operation-results.json",
                "duplicate operation schema",
            );
        }
    }
    if schemas.len() != operations.len() {
        return contract_error(
            "synapse-operation-results.json",
            "result schema coverage mismatch",
        );
    }
    Ok(schemas)
}

#[derive(Deserialize)]
pub(crate) struct LegacyBundle {
    pub(crate) operations: Vec<LegacyOperationBinding>,
}

#[derive(Deserialize)]
pub(crate) struct CanonicalBundle {
    pub(crate) classification_sha256: String,
    pub(crate) operations: Vec<OperationSpec>,
}

#[derive(Deserialize)]
pub(crate) struct DiagnosticBundle {
    pub(crate) classification_sha256: String,
    pub(crate) mappings: Vec<DiagnosticProjection>,
}

#[derive(Deserialize)]
pub(crate) struct ParameterBundle {
    pub(crate) classification_sha256: String,
    pub(crate) schemas: Vec<ParameterRecord>,
}

#[derive(Deserialize)]
pub(crate) struct ParameterRecord {
    operation_name: String,
    schema_id: SchemaId,
    schema: Value,
}

#[derive(Deserialize)]
pub(crate) struct ResultBundle {
    pub(crate) classification_sha256: String,
    pub(crate) schemas: Vec<ResultRecord>,
}

#[derive(Deserialize)]
pub(crate) struct ResultRecord {
    operation_name: String,
    schema_id: SchemaId,
    family: String,
    schema: Value,
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
