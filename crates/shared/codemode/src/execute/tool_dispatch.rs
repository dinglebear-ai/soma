use std::sync::Arc;

use serde_json::Value;

use super::budget::RunBudget;
use crate::host::{CodeModeHost, ExecCtx};
use crate::local_provider::{
    dispatch_local_provider, parse_local_provider_call, LocalProviderCall,
};
use crate::types::{
    CodeModeCaller, CodeModeExecutedCall, CodeModeSurface, ToolDescriptor, ToolScope, UiLink,
};
use crate::ToolError;

pub(crate) struct ToolCallContext<'a, H: CodeModeHost> {
    pub(crate) host: Option<&'a H>,
    pub(crate) entries: &'a [ToolDescriptor],
    pub(crate) caller: &'a CodeModeCaller,
    pub(crate) surface: CodeModeSurface,
    pub(crate) scope: &'a ToolScope,
    pub(crate) execution_id: &'a Option<Arc<str>>,
    pub(crate) ui_capture: &'a Arc<std::sync::Mutex<Option<UiLink>>>,
    pub(crate) calls: &'a mut Vec<CodeModeExecutedCall>,
}

pub(crate) async fn handle_tool_call<H: CodeModeHost>(
    ctx: &mut ToolCallContext<'_, H>,
    budget: &mut RunBudget,
    seq: u64,
    id: String,
    params: Value,
) -> Result<Value, ToolError> {
    budget.record_operation("tool call")?;
    let result = if let Some(call) = parse_local_provider_call(&id, params.clone())? {
        if !local_providers_allowed(ctx.caller, ctx.scope) {
            Err(ToolError::Forbidden {
                message: format!("Code Mode local provider `{id}` is not available in this scope"),
                required_scopes: vec!["soma:admin".to_string()],
            })
        } else {
            dispatch_scoped_local_provider(ctx.host, call).await
        }
    } else {
        call_catalog_tool(ctx, seq, &id, params.clone()).await
    };
    let result = result.map(|value| budget.cap_tool_result(value));
    match &result {
        Ok(value) => ctx.calls.push(CodeModeExecutedCall {
            id,
            params: Some(params),
            result: Some(value.clone()),
        }),
        Err(_) => ctx.calls.push(CodeModeExecutedCall {
            id,
            params: Some(params),
            result: None,
        }),
    }
    result
}

async fn call_catalog_tool<H: CodeModeHost>(
    ctx: &mut ToolCallContext<'_, H>,
    seq: u64,
    id: &str,
    params: Value,
) -> Result<Value, ToolError> {
    let host = ctx.host.ok_or_else(|| unknown_tool(id, ctx.entries))?;
    let descriptor = ctx
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| unknown_tool(id, ctx.entries))?;
    let outcome = call_host_tool_with_ctx(
        host,
        descriptor,
        params,
        ctx.caller,
        ctx.surface,
        ctx.scope,
        ExecCtx {
            seq,
            execution_id: ctx.execution_id.clone(),
            step_ordinal: None,
        },
    )
    .await?;
    if let Some(ui) = outcome.ui.clone() {
        if let Ok(mut guard) = ctx.ui_capture.lock() {
            *guard = Some(ui);
        }
    }
    Ok(outcome.value)
}

async fn dispatch_scoped_local_provider<H: CodeModeHost>(
    host: Option<&H>,
    call: LocalProviderCall,
) -> Result<Value, ToolError> {
    #[cfg(feature = "openapi")]
    if matches!(
        call.provider,
        crate::local_provider::LocalProviderName::Openapi
    ) {
        let host = host.ok_or_else(|| ToolError::UnknownInstance {
            message: "OpenAPI provider requires a Code Mode host".to_string(),
            valid: Vec::new(),
        })?;
        let registry = host
            .openapi_registry()
            .ok_or_else(|| ToolError::UnknownInstance {
                message: "Code Mode host has no OpenAPI registry".to_string(),
                valid: Vec::new(),
            })?;
        let client = host
            .openapi_http_client()
            .ok_or_else(|| ToolError::internal_message("Code Mode host has no OpenAPI client"))?;
        return crate::openapi_feature::dispatch_openapi_provider(
            &registry,
            &client,
            &call.method,
            call.params,
        )
        .await;
    }
    let _ = host;
    dispatch_local_provider(call).await
}

async fn call_host_tool_with_ctx<H: CodeModeHost>(
    host: &H,
    descriptor: &ToolDescriptor,
    params: Value,
    caller: &CodeModeCaller,
    surface: CodeModeSurface,
    scope: &ToolScope,
    ctx: ExecCtx,
) -> Result<crate::host::ToolCallOutcome, ToolError> {
    if !scope.allows(&descriptor.id) {
        return Err(ToolError::Forbidden {
            message: format!("Code Mode scope does not allow `{}`", descriptor.id),
            required_scopes: vec![descriptor.namespace.clone()],
        });
    }
    crate::schema::validate_code_mode_params_against_schema(&params, descriptor.schema.as_ref())?;
    host.call_tool(&descriptor.id, params, caller, surface, scope, ctx)
        .await
}

pub(crate) fn local_providers_allowed(caller: &CodeModeCaller, scope: &ToolScope) -> bool {
    matches!(scope, ToolScope::All)
        && (caller.capabilities.admin || caller.capabilities.trusted_local)
}

fn unknown_tool(id: &str, entries: &[ToolDescriptor]) -> ToolError {
    ToolError::UnknownAction {
        message: format!("unknown Code Mode tool `{id}`"),
        valid: entries.iter().map(|entry| entry.id.clone()).collect(),
        hint: Some(crate::broker::code_mode_unknown_tool_hint()),
    }
}
