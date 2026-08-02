use std::path::PathBuf;

use chrono::DateTime;
use serde_json::{Map, Value};

use crate::ExecutionError;

pub(crate) fn object(parameters: &Value) -> Result<&Map<String, Value>, ExecutionError> {
    parameters
        .as_object()
        .ok_or_else(|| invalid("$", "expected an object"))
}

pub(crate) fn required_str<'a>(
    parameters: &'a Value,
    field: &str,
) -> Result<&'a str, ExecutionError> {
    optional_str(parameters, field)?.ok_or_else(|| invalid(field, "required string is missing"))
}

pub(crate) fn optional_str<'a>(
    parameters: &'a Value,
    field: &str,
) -> Result<Option<&'a str>, ExecutionError> {
    match object(parameters)?.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(field, "expected a string")),
    }
}

pub(crate) fn bool_or(
    parameters: &Value,
    field: &str,
    default: bool,
) -> Result<bool, ExecutionError> {
    match object(parameters)?.get(field) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(invalid(field, "expected a boolean")),
    }
}

pub(crate) fn u32_or(parameters: &Value, field: &str, default: u32) -> Result<u32, ExecutionError> {
    optional_u32(parameters, field).map(|value| value.unwrap_or(default))
}

pub(crate) fn optional_u32(parameters: &Value, field: &str) -> Result<Option<u32>, ExecutionError> {
    match object(parameters)?.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| invalid(field, "expected an unsigned 32-bit integer")),
        Some(_) => Err(invalid(field, "expected an integer")),
    }
}

pub(crate) fn required_path(parameters: &Value, field: &str) -> Result<PathBuf, ExecutionError> {
    Ok(PathBuf::from(required_str(parameters, field)?))
}

pub(crate) fn parse_time_spec(value: &str) -> Result<i64, ExecutionError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid("time", "time expression is empty"));
    }
    if value.chars().all(|character| character.is_ascii_digit()) {
        return value
            .parse::<i64>()
            .map_err(|error| invalid("time", &error.to_string()));
    }
    if let Some(unit) = value.chars().last()
        && matches!(unit, 's' | 'm' | 'h' | 'd')
    {
        let digits = &value[..value.len() - 1];
        if !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()) {
            let count = digits
                .parse::<i64>()
                .map_err(|error| invalid("time", &error.to_string()))?;
            let multiplier = match unit {
                's' => 1,
                'm' => 60,
                'h' => 3_600,
                _ => 86_400,
            };
            return Ok(chrono::Utc::now()
                .timestamp()
                .saturating_sub(count.saturating_mul(multiplier)));
        }
    }
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp())
        .map_err(|error| invalid("time", &format!("invalid RFC3339 time: {error}")))
}

pub(crate) fn invalid(field: &str, message: &str) -> ExecutionError {
    ExecutionError::InvalidParameter {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}
