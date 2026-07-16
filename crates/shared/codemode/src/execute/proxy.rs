use serde_json::Value;

use crate::host::CodeModeHost;
use crate::preamble::{
    generate_discovery_js, generate_js_proxy_from_catalog, generate_local_provider_js,
};
use crate::types::{CodeModeCaller, CodeModeSurface, ToolDescriptor, ToolScope};
use crate::ToolError;

pub(crate) async fn load_entries<H: CodeModeHost>(
    host: Option<&H>,
    caller: &CodeModeCaller,
    surface: CodeModeSurface,
    scope: &ToolScope,
) -> Result<Vec<ToolDescriptor>, ToolError> {
    match host {
        Some(host) => Ok(host
            .list_tools(caller, surface, scope, true, true)
            .await?
            .entries
            .iter()
            .filter(|entry| scope.allows(&entry.id))
            .cloned()
            .collect()),
        None => Ok(Vec::new()),
    }
}

pub(crate) fn build_proxy(
    entries: &[ToolDescriptor],
    blend_weight: f32,
) -> Result<String, ToolError> {
    let values = entries
        .iter()
        .map(|entry| serde_json::to_value(entry).map_err(serialize_error))
        .collect::<Result<Vec<Value>, _>>()?;
    let mut proxy = String::new();
    proxy.push_str(generate_local_provider_js());
    #[cfg(feature = "openapi")]
    proxy.push_str(crate::preamble::generate_openapi_provider_js());
    proxy.push_str(
        &generate_discovery_js(&values, blend_weight).map_err(ToolError::internal_message)?,
    );
    proxy.push_str(&generate_js_proxy_from_catalog(entries).map_err(ToolError::internal_message)?);
    proxy.push_str(
        r#"
codemode.run = (name, input = {}) => globalThis.__somaRunSnippet(name, input);
codemode.step = (name, fn) => globalThis.__somaCodemodeStep(name, fn);
codemode.search = async (query = "") => {
  const q = String(query || "").toLowerCase();
  return globalThis.__codemodeDiscovery.filter((entry) => JSON.stringify(entry).toLowerCase().includes(q));
};
codemode.describe = async (query = "") => ({
  tools: (await codemode.search(query)).map((entry) => ({
    id: entry.id,
    signature: entry.signature,
    dts: entry.dts,
    description: entry.description
  }))
});
"#,
    );
    Ok(proxy)
}

fn serialize_error(error: serde_json::Error) -> ToolError {
    ToolError::internal_message(format!("failed to serialize Code Mode value: {error}"))
}
