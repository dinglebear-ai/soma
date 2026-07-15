use serde_json::Value;

use crate::host::{CodeModeHost, ExecCtx, ToolCallOutcome};
use crate::schema::validate_code_mode_params_against_schema;
use crate::types::{CodeModeCaller, CodeModeSurface, ToolDescriptor, ToolScope};
use crate::ToolError;

pub async fn call_host_tool<H: CodeModeHost>(
    host: &H,
    descriptor: &ToolDescriptor,
    params: Value,
    caller: &CodeModeCaller,
    surface: CodeModeSurface,
    scope: &ToolScope,
) -> Result<ToolCallOutcome, ToolError> {
    if !scope.allows(&descriptor.id) {
        return Err(ToolError::Forbidden {
            message: format!("Code Mode scope does not allow `{}`", descriptor.id),
            required_scopes: vec![descriptor.namespace.clone()],
        });
    }
    validate_code_mode_params_against_schema(&params, descriptor.schema.as_ref())?;
    host.call_tool(
        &descriptor.id,
        params,
        caller,
        surface,
        scope,
        ExecCtx::none(),
    )
    .await
}
