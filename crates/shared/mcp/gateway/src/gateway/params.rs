use std::collections::BTreeMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::config::UpstreamConfig;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParamsError {
    #[error("params must be a JSON object")]
    MustBeObject,
    #[error("field `{0}` is required")]
    MissingField(&'static str),
    #[error("field `{0}` must be a string")]
    StringField(&'static str),
    #[error("field `{0}` must be an array of strings")]
    StringArrayField(&'static str),
    #[error("field `{0}` must be an object with string values")]
    StringMapField(&'static str),
}

pub fn object_params(params: &Value) -> Result<&Map<String, Value>, ParamsError> {
    params.as_object().ok_or(ParamsError::MustBeObject)
}

pub fn string_param(
    params: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<String>, ParamsError> {
    params
        .get(field)
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or(ParamsError::StringField(field))
        })
        .transpose()
}

pub fn upstream_config_from_params(params: &Value) -> Result<UpstreamConfig, ParamsError> {
    let params = object_params(params)?;
    let mut config = parsed_upstream_config(params, required_string_param(params, "name")?)?;
    config.proxy_resources = params
        .get("proxy_resources")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    config.proxy_prompts = params
        .get("proxy_prompts")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Ok(config)
}

pub fn test_upstream_config_from_params(params: &Value) -> Result<UpstreamConfig, ParamsError> {
    let params = object_params(params)?;
    let name = string_param(params, "name")?.unwrap_or_else(|| "test".to_owned());
    parsed_upstream_config(params, name)
}

fn parsed_upstream_config(
    params: &Map<String, Value>,
    name: String,
) -> Result<UpstreamConfig, ParamsError> {
    Ok(UpstreamConfig {
        name,
        url: string_param(params, "url")?,
        command: string_param(params, "command")?,
        args: string_array_param(params, "args")?.unwrap_or_default(),
        env: env_param(params)?,
        ..UpstreamConfig::default()
    })
}

pub fn required_string_param(
    params: &Map<String, Value>,
    field: &'static str,
) -> Result<String, ParamsError> {
    string_param(params, field)?.ok_or(ParamsError::MissingField(field))
}

fn string_array_param(
    params: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<Vec<String>>, ParamsError> {
    params
        .get(field)
        .map(|value| {
            value
                .as_array()
                .ok_or(ParamsError::StringArrayField(field))?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(ToOwned::to_owned)
                        .ok_or(ParamsError::StringArrayField(field))
                })
                .collect()
        })
        .transpose()
}

fn env_param(params: &Map<String, Value>) -> Result<BTreeMap<String, String>, ParamsError> {
    params
        .get("env")
        .map(|value| {
            value
                .as_object()
                .ok_or(ParamsError::StringMapField("env"))?
                .iter()
                .map(|(key, value)| {
                    value
                        .as_str()
                        .map(|value| (key.clone(), value.to_owned()))
                        .ok_or(ParamsError::StringMapField("env"))
                })
                .collect()
        })
        .transpose()
        .map(|value| value.unwrap_or_default())
}

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
