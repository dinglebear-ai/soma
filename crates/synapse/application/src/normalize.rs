use serde_json::{Map, Value};
use soma_ops::OperationName;

use crate::{CompatibilityError, LegacyPresentation, LegacyTool, SynapseCatalog};

const SURFACE_FIELDS: [&str; 4] = ["action", "subaction", "response_format", "format"];

/// Canonical operation request produced from one legacy Flux or Scout input.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedOperationRequest {
    operation: OperationName,
    parameters: Value,
    presentation: LegacyPresentation,
    required_scope: Option<String>,
    legacy_name: String,
}

impl NormalizedOperationRequest {
    /// Returns the resolved canonical operation.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns schema-validated canonical parameters.
    #[must_use]
    pub fn parameters(&self) -> &Value {
        &self.parameters
    }

    /// Returns requested legacy presentation.
    #[must_use]
    pub const fn presentation(&self) -> LegacyPresentation {
        self.presentation
    }

    /// Returns the product authorization scope required by the legacy route.
    #[must_use]
    pub fn required_scope(&self) -> Option<&str> {
        self.required_scope.as_deref()
    }

    /// Returns the historical operation name.
    #[must_use]
    pub fn legacy_name(&self) -> &str {
        &self.legacy_name
    }
}

impl SynapseCatalog {
    /// Normalizes a legacy Flux or Scout input into canonical parameters.
    pub fn normalize_legacy_request(
        &self,
        tool: LegacyTool,
        input: &Value,
    ) -> Result<NormalizedOperationRequest, CompatibilityError> {
        let object = input
            .as_object()
            .ok_or_else(|| CompatibilityError::InvalidLegacyRequest {
                field: "$".into(),
                message: "expected an object".into(),
            })?;
        let action = required_string(object, "action")?;
        let subaction = optional_string(object, "subaction")?;
        let binding = self.binding(tool, action, subaction).ok_or_else(|| {
            CompatibilityError::UnknownLegacyOperation {
                tool: tool.as_str(),
                action: action.to_owned(),
                subaction: subaction.map(str::to_owned),
            }
        })?;
        let operation = binding.canonical_name().clone();
        let contract = self
            .parameter_schema(&operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?;
        let properties = contract
            .schema()
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(|| CompatibilityError::EmbeddedContract {
                artifact: "synapse-operation-parameters.json",
                message: format!("{operation} has no properties object"),
            })?;

        let mut parameters = Map::new();
        for (field, value) in object {
            if SURFACE_FIELDS.contains(&field.as_str()) {
                continue;
            }
            if !properties.contains_key(field) {
                return Err(CompatibilityError::UnknownField {
                    operation,
                    field: field.clone(),
                });
            }
            parameters.insert(field.clone(), value.clone());
        }
        let parameters = Value::Object(parameters);
        contract.validate(&operation, "parameter", &parameters)?;

        Ok(NormalizedOperationRequest {
            operation,
            parameters,
            presentation: presentation(object)?,
            required_scope: binding.scope().map(str::to_owned),
            legacy_name: binding.legacy_name().to_owned(),
        })
    }

    /// Validates already canonical parameters for one operation.
    pub fn validate_parameters(
        &self,
        operation: &OperationName,
        parameters: &Value,
    ) -> Result<(), CompatibilityError> {
        self.parameter_schema(operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?
            .validate(operation, "parameter", parameters)
    }
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, CompatibilityError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CompatibilityError::InvalidLegacyRequest {
            field: field.to_owned(),
            message: "expected a non-empty string".into(),
        })
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, CompatibilityError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(_) => Err(CompatibilityError::InvalidLegacyRequest {
            field: field.to_owned(),
            message: "expected a non-empty string".into(),
        }),
    }
}

fn presentation(object: &Map<String, Value>) -> Result<LegacyPresentation, CompatibilityError> {
    let format = optional_string(object, "format")?;
    let response = optional_string(object, "response_format")?;
    if let (Some(format), Some(response)) = (format, response)
        && format != response
    {
        return Err(CompatibilityError::ConflictingPresentation);
    }
    match response.or(format).unwrap_or("markdown") {
        "markdown" => Ok(LegacyPresentation::Markdown),
        "json" => Ok(LegacyPresentation::Json),
        other => Err(CompatibilityError::InvalidPresentation(other.to_owned())),
    }
}

#[cfg(test)]
#[path = "normalize_tests.rs"]
mod tests;
