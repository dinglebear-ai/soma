use crate::error::OpenApiError;

#[derive(Debug, Clone)]
pub struct OperationDescriptor {
    pub operation_id: String,
    pub method: reqwest::Method,
    pub path_template: String,
}

const HTTP_METHOD_KEYS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

pub fn convert_spec(
    label: &str,
    spec_json: &str,
    allowed: &[String],
) -> Result<Vec<OperationDescriptor>, OpenApiError> {
    let value: serde_json::Value =
        serde_json::from_str(spec_json).map_err(|_| OpenApiError::SpecParse {
            label: label.to_string(),
        })?;

    let paths = value
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| OpenApiError::SpecParse {
            label: label.to_string(),
        })?;

    let mut out = Vec::new();
    for (path_template, path_item) in paths {
        let Some(path_item) = path_item.as_object() else {
            continue;
        };
        for method_key in HTTP_METHOD_KEYS {
            let Some(operation) = path_item
                .get(*method_key)
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };
            let Some(operation_id) = operation
                .get("operationId")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            if !allowed.iter().any(|allowed| allowed == operation_id) {
                continue;
            }
            validate_operation_path_template(label, path_template)?;
            out.push(OperationDescriptor {
                operation_id: operation_id.to_string(),
                method: parse_method(label, operation_id, method_key),
                path_template: path_template.clone(),
            });
        }
    }
    Ok(out)
}

fn validate_operation_path_template(label: &str, path_template: &str) -> Result<(), OpenApiError> {
    if !path_template.starts_with('/') || path_template.contains('\\') {
        return Err(OpenApiError::SpecParse {
            label: label.to_string(),
        });
    }
    if path_template
        .split('/')
        .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(OpenApiError::SpecParse {
            label: label.to_string(),
        });
    }
    Ok(())
}

fn parse_method(label: &str, operation_id: &str, raw: &str) -> reqwest::Method {
    raw.to_ascii_uppercase()
        .parse::<reqwest::Method>()
        .unwrap_or_else(|_| {
            tracing::warn!(
                service = "openapi",
                label = %label,
                operation = %operation_id,
                "openapi: unparseable HTTP method in spec, falling back to GET"
            );
            reqwest::Method::GET
        })
}
