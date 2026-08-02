use serde_json::Value;
use soma_ops::OperationName;

use crate::{CompatibilityError, SynapseCatalog};

impl SynapseCatalog {
    /// Validates one canonical result payload against its checked-in schema.
    pub fn validate_result(
        &self,
        operation: &OperationName,
        result: &Value,
    ) -> Result<(), CompatibilityError> {
        self.result_schema(operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?
            .validate(operation, "result", result)
    }
}

#[cfg(test)]
#[path = "catalog_validation_tests.rs"]
mod tests;
