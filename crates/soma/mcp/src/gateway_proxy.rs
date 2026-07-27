use rmcp::model::{
    CallToolResponse, CallToolResult, CreateTaskResult, GetPromptResponse, GetPromptResult,
    InputRequiredResult, Prompt, ReadResourceResponse, ReadResourceResult, Resource, Tool,
};
use serde_json::{json, Map, Value};
use soma_application::{
    ApplicationError, ExecutionContext, GatewayMcpOutcome, GatewayMcpRoundTrip, GatewayRouteScope,
    SomaApplication,
};
use soma_mcp_server::protocol::{
    prompt_from_descriptor, resource_from_descriptor, tool_from_descriptor,
};

pub async fn list_tools_for_subject_and_scope(
    application: &SomaApplication,
    scope: Option<&GatewayRouteScope>,
    context: &ExecutionContext,
) -> Result<Vec<Tool>, rmcp::ErrorData> {
    let routes = application
        .gateway_mcp_tools(scope, context)
        .await
        .map_err(protocol_error)?;
    Ok(routes
        .into_iter()
        .map(|route| {
            tool_from_descriptor(
                route.name,
                route.description,
                route.input_schema,
                route.output_schema,
                route.destructive,
            )
        })
        .collect())
}

pub async fn tool_requires_confirmation(
    application: &SomaApplication,
    name: &str,
    scope: Option<&GatewayRouteScope>,
    context: &ExecutionContext,
) -> Result<Option<bool>, rmcp::ErrorData> {
    let routes = application
        .gateway_mcp_tools(scope, context)
        .await
        .map_err(protocol_error)?;
    Ok(routes
        .into_iter()
        .find(|route| route.name == name)
        .map(|route| route.destructive))
}

pub async fn call_tool_for_subject_and_scope(
    application: &SomaApplication,
    name: &str,
    args: Option<Map<String, Value>>,
    round_trip: GatewayMcpRoundTrip,
    scope: Option<&GatewayRouteScope>,
    context: &ExecutionContext,
) -> Option<CallToolResponse> {
    let params = Value::Object(args.unwrap_or_default());
    match application
        .gateway_call_mcp_tool_once(name, params, round_trip, scope, context)
        .await
    {
        Ok(Some(outcome)) => Some(call_tool_response(outcome).unwrap_or_else(|error| {
            CallToolResponse::Complete(CallToolResult::structured_error(json!({
                "kind": "mcp_proxy_decode_error",
                "schema_version": 1,
                "code": "invalid_upstream_result",
                "tool": name,
                "message": error.to_string(),
                "retryable": false,
                "remediation": "Update the upstream server to emit a valid MCP result object.",
            })))
        })),
        Ok(None) => None,
        Err(error) => Some(CallToolResponse::Complete(
            CallToolResult::structured_error(error_payload("upstream_call_failed", name, error)),
        )),
    }
}

pub async fn list_resources_for_subject_and_scope(
    application: &SomaApplication,
    scope: Option<&GatewayRouteScope>,
    context: &ExecutionContext,
) -> Result<Vec<Resource>, rmcp::ErrorData> {
    let routes = application
        .gateway_mcp_resources(scope, context)
        .await
        .map_err(protocol_error)?;
    Ok(routes
        .into_iter()
        .map(|route| {
            let name = route.name.unwrap_or_else(|| route.native_uri.clone());
            resource_from_descriptor(route.uri, name)
        })
        .collect())
}

pub async fn read_resource_for_subject_and_scope(
    application: &SomaApplication,
    uri: &str,
    round_trip: GatewayMcpRoundTrip,
    scope: Option<&GatewayRouteScope>,
    context: &ExecutionContext,
) -> Result<Option<ReadResourceResponse>, rmcp::ErrorData> {
    match application
        .gateway_read_mcp_resource_once(uri, round_trip, scope, context)
        .await
    {
        Ok(Some(outcome)) => read_resource_response(outcome).map(Some),
        Ok(None) => Ok(None),
        Err(error) => Err(protocol_error(error)),
    }
}

pub async fn list_prompts_for_subject_and_scope(
    application: &SomaApplication,
    scope: Option<&GatewayRouteScope>,
    context: &ExecutionContext,
) -> Result<Vec<Prompt>, rmcp::ErrorData> {
    let routes = application
        .gateway_mcp_prompts(scope, context)
        .await
        .map_err(protocol_error)?;
    Ok(routes
        .into_iter()
        .map(|route| prompt_from_descriptor(route.name, route.description.as_deref()))
        .collect())
}

pub async fn get_prompt_for_subject_and_scope(
    application: &SomaApplication,
    name: &str,
    arguments: Option<Map<String, Value>>,
    round_trip: GatewayMcpRoundTrip,
    scope: Option<&GatewayRouteScope>,
    context: &ExecutionContext,
) -> Result<Option<GetPromptResponse>, rmcp::ErrorData> {
    match application
        .gateway_get_mcp_prompt_once(name, arguments, round_trip, scope, context)
        .await
    {
        Ok(Some(outcome)) => get_prompt_response(outcome).map(Some),
        Ok(None) => Ok(None),
        Err(error) => Err(protocol_error(error)),
    }
}

fn call_tool_response(outcome: GatewayMcpOutcome) -> Result<CallToolResponse, serde_json::Error> {
    match outcome {
        GatewayMcpOutcome::Complete(value) => {
            serde_json::from_value::<CallToolResult>(value).map(CallToolResponse::Complete)
        }
        GatewayMcpOutcome::InputRequired(value) => {
            serde_json::from_value::<InputRequiredResult>(value)
                .map(CallToolResponse::InputRequired)
        }
        GatewayMcpOutcome::Task(value) => {
            serde_json::from_value::<CreateTaskResult>(value).map(CallToolResponse::Task)
        }
    }
}

fn read_resource_response(
    outcome: GatewayMcpOutcome,
) -> Result<ReadResourceResponse, rmcp::ErrorData> {
    match outcome {
        GatewayMcpOutcome::Complete(value) => serde_json::from_value::<ReadResourceResult>(value)
            .map(ReadResourceResponse::Complete)
            .map_err(proxy_decode_error),
        GatewayMcpOutcome::InputRequired(value) => {
            serde_json::from_value::<InputRequiredResult>(value)
                .map(ReadResourceResponse::InputRequired)
                .map_err(proxy_decode_error)
        }
        GatewayMcpOutcome::Task(_) => Err(rmcp::ErrorData::internal_error(
            "upstream returned a task result for resources/read",
            None,
        )),
    }
}

fn get_prompt_response(outcome: GatewayMcpOutcome) -> Result<GetPromptResponse, rmcp::ErrorData> {
    match outcome {
        GatewayMcpOutcome::Complete(value) => serde_json::from_value::<GetPromptResult>(value)
            .map(GetPromptResponse::Complete)
            .map_err(proxy_decode_error),
        GatewayMcpOutcome::InputRequired(value) => {
            serde_json::from_value::<InputRequiredResult>(value)
                .map(GetPromptResponse::InputRequired)
                .map_err(proxy_decode_error)
        }
        GatewayMcpOutcome::Task(_) => Err(rmcp::ErrorData::internal_error(
            "upstream returned a task result for prompts/get",
            None,
        )),
    }
}

fn proxy_decode_error(error: serde_json::Error) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(
        "invalid MCP result returned by upstream",
        Some(json!({
            "kind": "mcp_proxy_decode_error",
            "schema_version": 1,
            "code": "invalid_upstream_result",
            "message": error.to_string(),
            "retryable": false,
            "remediation": "Update the upstream server to emit a valid MCP result object.",
        })),
    )
}

fn protocol_error(error: ApplicationError) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(
        error.to_string(),
        Some(error_payload("gateway_proxy_failed", "gateway", error)),
    )
}

fn error_payload(code: &str, tool: &str, error: ApplicationError) -> Value {
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
