use serde_json::{Map, Value};

use crate::ToolError;

use super::store::{SnippetInfo, SnippetInputType};

pub fn bind_snippet_input(info: &SnippetInfo, input: Value) -> Result<Value, ToolError> {
    let object = input.as_object().cloned().unwrap_or_default();
    let mut bound = Map::new();
    for (name, spec) in &info.inputs {
        match object.get(name) {
            Some(value) if input_type_matches(value, &spec.input_type) => {
                bound.insert(name.clone(), value.clone());
            }
            Some(_) => {
                return Err(ToolError::InvalidParam {
                    message: format!("snippet input `{name}` has wrong type"),
                    param: name.clone(),
                });
            }
            None if spec.required => {
                return Err(ToolError::MissingParam {
                    message: format!("snippet input `{name}` is required"),
                    param: name.clone(),
                });
            }
            None => {}
        }
    }
    Ok(Value::Object(bound))
}

fn input_type_matches(value: &Value, input_type: &SnippetInputType) -> bool {
    match input_type {
        SnippetInputType::String => value.is_string(),
        SnippetInputType::Number => value.is_number(),
        SnippetInputType::Boolean => value.is_boolean(),
        SnippetInputType::Json => true,
    }
}
