use std::{borrow::Cow, sync::Arc};

use rmcp::model::{
    CallToolResult, GetPromptResult, Prompt, ReadResourceResult, Resource, Tool, ToolAnnotations,
};
use serde_json::{json, Map, Value};
use soma_gateway::gateway::{manager::GatewayManagerError, protected_routes::ProtectedRouteScope};
use soma_runtime::server::GatewayProductState;

pub async fn list_tools_for_subject_and_scope(
    gateway: &GatewayProductState,
    subject: Option<&str>,
    scope: Option<&ProtectedRouteScope>,
) -> Result<Vec<Tool>, rmcp::ErrorData> {
    let routes = gateway
        .tool_routes_for_subject_and_scope(subject, scope)
        .await
        .map_err(protocol_error)?;
    Ok(routes
        .into_iter()
        .map(|route| {
            let mut tool = Tool::new_with_raw(
                route.name,
                route.descriptor.description.map(Cow::Owned),
                schema_object(route.descriptor.input_schema),
            );
            if let Some(output_schema) = schema_object_opt(route.descriptor.output_schema) {
                tool = tool.with_raw_output_schema(output_schema);
            }
            tool.with_annotations(ToolAnnotations::new().destructive(route.descriptor.destructive))
        })
        .collect())
}

pub async fn call_tool_for_subject_and_scope(
    gateway: &GatewayProductState,
    name: &str,
    args: Option<Map<String, Value>>,
    subject: Option<&str>,
    scope: Option<&ProtectedRouteScope>,
) -> Option<CallToolResult> {
    let params = Value::Object(args.unwrap_or_default());
    match gateway
        .call_mcp_tool_for_subject_and_scope(name, params, subject, scope)
        .await
    {
        Ok(Some(value)) => Some(CallToolResult::structured(value)),
        Ok(None) => None,
        Err(error) => Some(CallToolResult::structured_error(error_payload(
            "upstream_call_failed",
            name,
            error,
        ))),
    }
}

pub async fn list_resources_for_subject_and_scope(
    gateway: &GatewayProductState,
    subject: Option<&str>,
    scope: Option<&ProtectedRouteScope>,
) -> Result<Vec<Resource>, rmcp::ErrorData> {
    let routes = gateway
        .resource_routes_for_subject_and_scope(subject, scope)
        .await
        .map_err(protocol_error)?;
    Ok(routes
        .into_iter()
        .map(|route| {
            Resource::new(
                route.uri,
                route
                    .descriptor
                    .name
                    .unwrap_or_else(|| route.native_uri.clone()),
            )
        })
        .collect())
}

pub async fn read_resource_for_subject_and_scope(
    gateway: &GatewayProductState,
    uri: &str,
    subject: Option<&str>,
    scope: Option<&ProtectedRouteScope>,
) -> Result<Option<ReadResourceResult>, rmcp::ErrorData> {
    match gateway
        .read_mcp_resource_for_subject_and_scope(uri, subject, scope)
        .await
    {
        Ok(Some(value)) => serde_json::from_value(value)
            .map(Some)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None)),
        Ok(None) => Ok(None),
        Err(error) => Err(protocol_error(error)),
    }
}

pub async fn list_prompts_for_subject_and_scope(
    gateway: &GatewayProductState,
    subject: Option<&str>,
    scope: Option<&ProtectedRouteScope>,
) -> Result<Vec<Prompt>, rmcp::ErrorData> {
    let routes = gateway
        .prompt_routes_for_subject_and_scope(subject, scope)
        .await
        .map_err(protocol_error)?;
    Ok(routes
        .into_iter()
        .map(|route| Prompt::new(route.name, route.descriptor.description.as_deref(), None))
        .collect())
}

pub async fn get_prompt_for_subject_and_scope(
    gateway: &GatewayProductState,
    name: &str,
    arguments: Option<Map<String, Value>>,
    subject: Option<&str>,
    scope: Option<&ProtectedRouteScope>,
) -> Result<Option<GetPromptResult>, rmcp::ErrorData> {
    match gateway
        .get_mcp_prompt_for_subject_and_scope(name, arguments, subject, scope)
        .await
    {
        Ok(Some(value)) => serde_json::from_value(value)
            .map(Some)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None)),
        Ok(None) => Ok(None),
        Err(error) => Err(protocol_error(error)),
    }
}

fn schema_object(value: Option<Value>) -> Arc<Map<String, Value>> {
    schema_object_opt(value).unwrap_or_else(|| {
        Arc::new(Map::from_iter([(
            "type".to_owned(),
            Value::String("object".to_owned()),
        )]))
    })
}

fn schema_object_opt(value: Option<Value>) -> Option<Arc<Map<String, Value>>> {
    match value {
        Some(Value::Object(map)) => Some(Arc::new(map)),
        _ => None,
    }
}

fn protocol_error(error: GatewayManagerError) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(
        error.to_string(),
        Some(error_payload("gateway_proxy_failed", "gateway", error)),
    )
}

fn error_payload(code: &str, tool: &str, error: GatewayManagerError) -> Value {
    json!({
        "kind": "mcp_tool_error",
        "schema_version": 1,
        "code": code,
        "tool": tool,
        "message": error.to_string(),
        "retryable": true,
        "remediation": "Check the gateway upstream configuration and retry.",
    })
}

#[cfg(test)]
#[path = "gateway_proxy_tests.rs"]
mod tests;
