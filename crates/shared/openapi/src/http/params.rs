use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::error::OpenApiError;
use crate::registry::OperationHandle;

const PATH_SEGMENT_ENCODE: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'~');

pub(crate) fn build_url_with_params(
    op: &OperationHandle,
    params: &serde_json::Value,
) -> Result<(Vec<String>, url::Url), OpenApiError> {
    let mut consumed = Vec::new();
    let mut path = String::with_capacity(op.path_template.len());
    let mut chars = op.path_template.chars();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut name = String::new();
            for nested in chars.by_ref() {
                if nested == '}' {
                    break;
                }
                name.push(nested);
            }
            path.push_str(&path_param_value(op, params, &name)?);
            consumed.push(name);
        } else {
            path.push(ch);
        }
    }

    let mut base = op.base_url.clone();
    ensure_base_path_prefix(&mut base);
    let joined =
        base.join(path.trim_start_matches('/'))
            .map_err(|_| OpenApiError::UpstreamRequest {
                label: op.operation_id.clone(),
            })?;
    if !path_is_under_base(&base, &joined) {
        return Err(OpenApiError::RequestBlockedPrivateAddr {
            label: op.operation_id.clone(),
        });
    }
    Ok((consumed, joined))
}

fn ensure_base_path_prefix(base: &mut url::Url) -> String {
    let base_path = base.path().to_string();
    if base_path.ends_with('/') {
        base_path
    } else {
        let with_slash = format!("{base_path}/");
        base.set_path(&with_slash);
        with_slash
    }
}

fn path_is_under_base(base: &url::Url, joined: &url::Url) -> bool {
    let base_segments = path_segments(base);
    if base_segments.is_empty() {
        return true;
    }
    let joined_segments = path_segments(joined);
    joined_segments.starts_with(&base_segments)
}

fn path_segments(url: &url::Url) -> Vec<&str> {
    url.path_segments()
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn path_param_value(
    op: &OperationHandle,
    params: &serde_json::Value,
    name: &str,
) -> Result<String, OpenApiError> {
    let raw = params
        .get(name)
        .ok_or_else(|| invalid_path_param(op, name))?;
    let value = match raw {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        _ => return Err(invalid_path_param(op, name)),
    };
    if value.is_empty() || value == "." || value == ".." {
        return Err(invalid_path_param(op, name));
    }
    Ok(utf8_percent_encode(&value, PATH_SEGMENT_ENCODE).to_string())
}

fn invalid_path_param(op: &OperationHandle, name: &str) -> OpenApiError {
    OpenApiError::InvalidPathParam {
        label: op.operation_id.clone(),
        param: name.to_string(),
    }
}

pub(crate) fn inject_credential(
    request: reqwest::RequestBuilder,
    op: &OperationHandle,
) -> reqwest::RequestBuilder {
    match &op.credential {
        Some(crate::config::OpenApiCredential::BearerToken(token)) => request.bearer_auth(token),
        Some(crate::config::OpenApiCredential::ApiKey { header, value }) => {
            request.header(header, value)
        }
        None => request,
    }
}

pub(crate) fn apply_query(
    mut url: url::Url,
    op: &OperationHandle,
    params: &serde_json::Value,
    used_path_params: &[String],
) -> url::Url {
    if is_body_method(&op.method) {
        return url;
    }
    let remaining = remaining_params(params, used_path_params);
    if remaining.is_empty() {
        return url;
    }
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in &remaining {
            pairs.append_pair(key, &json_scalar_to_string(value));
        }
    }
    url
}

pub(crate) fn apply_body(
    request: reqwest::RequestBuilder,
    op: &OperationHandle,
    params: &serde_json::Value,
    used_path_params: &[String],
) -> reqwest::RequestBuilder {
    if !is_body_method(&op.method) {
        return request;
    }
    let remaining = remaining_params(params, used_path_params);
    if remaining.is_empty() {
        return request;
    }
    request.json(&serde_json::Value::Object(remaining))
}

fn remaining_params(
    params: &serde_json::Value,
    used_path_params: &[String],
) -> serde_json::Map<String, serde_json::Value> {
    match params {
        serde_json::Value::Object(map) => map
            .iter()
            .filter(|(key, _)| !used_path_params.contains(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        _ => serde_json::Map::new(),
    }
}

fn json_scalar_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn is_body_method(method: &reqwest::Method) -> bool {
    matches!(
        *method,
        reqwest::Method::POST | reqwest::Method::PUT | reqwest::Method::PATCH
    )
}
