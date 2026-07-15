use std::collections::BTreeMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::config::UpstreamConfig;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParamsError {
    #[error("params must be a JSON object")]
    MustBeObject,
    #[error("field `{0}` must be a string")]
    StringField(&'static str),
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
    Ok(UpstreamConfig {
        name: string_param(params, "name")?.unwrap_or_else(|| "pending".to_owned()),
        url: string_param(params, "url")?,
        command: string_param(params, "command")?,
        env: env_param(params),
        proxy_resources: params
            .get("proxy_resources")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        proxy_prompts: params
            .get("proxy_prompts")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        ..UpstreamConfig::default()
    })
}

fn env_param(params: &Map<String, Value>) -> BTreeMap<String, String> {
    params
        .get("env")
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_owned()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
