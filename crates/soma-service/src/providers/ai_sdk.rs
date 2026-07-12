use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde_json::{json, Value};
use soma_contracts::providers::{ProviderCatalog, ProviderTool};
use tokio::time::Instant;

use crate::{
    provider_errors::{redact_public, ProviderError},
    provider_registry::{Provider, ProviderCall, ProviderOutput},
    providers::sidecar::{
        collect_provider_env, output_exceeded_message, run_bounded_sidecar, SidecarError,
    },
};

#[derive(Clone)]
pub struct AiSdkProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
}

impl AiSdkProvider {
    pub fn new(path: PathBuf, catalog: ProviderCatalog) -> Self {
        Self { path, catalog }
    }

    pub fn arc(path: PathBuf, catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::new(path, catalog))
    }
}

#[async_trait]
impl Provider for AiSdkProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let tool = self.tool(&call)?;
        let runtime = SidecarRuntime::from_tool(&self.catalog, tool, &call)?;
        let input = serde_json::to_vec(&json!({
            "action": call.action,
            "params": call.params,
            "provider": self.catalog.provider.name,
        }))
        .map_err(|error| ProviderError::execution(&self.catalog.provider.name, "", error))?;

        if input.len() > runtime.max_input_bytes {
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "ai_sdk_input_too_large",
                format!("AI SDK input exceeds {} bytes", runtime.max_input_bytes),
            ));
        }

        let wrapper = SidecarWrapper::new(&self.path).map_err(|error| {
            ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
        })?;
        let started = Instant::now();
        let sidecar = match run_bounded_sidecar(
            &runtime.command,
            &["--input-type=module", "--eval", wrapper.source()],
            runtime.env,
            &input,
            runtime.timeout_ms,
            runtime.max_output_bytes,
        )
        .await
        {
            Ok(sidecar) => sidecar,
            Err(SidecarError::Timeout) => {
                return Err(ProviderError::new(
                    "ai_sdk_provider_timeout",
                    &self.catalog.provider.name,
                    Some(call.action.clone()),
                    format!("AI SDK provider exceeded {}ms timeout", runtime.timeout_ms),
                    "Increase tool.limits.timeout_ms or fix the provider handler.",
                ));
            }
            Err(error) => {
                return Err(ProviderError::execution(
                    &self.catalog.provider.name,
                    call.action.clone(),
                    error,
                ));
            }
        };
        let output = sidecar.output;

        tracing::debug!(
            provider = %self.catalog.provider.name,
            action = %call.action,
            elapsed_ms = started.elapsed().as_millis(),
            "AI SDK provider sidecar completed"
        );

        if sidecar.stdout_exceeded || sidecar.stderr_exceeded {
            let stream = if sidecar.stdout_exceeded {
                "stdout"
            } else {
                "stderr"
            };
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "ai_sdk_output_too_large",
                output_exceeded_message(stream, runtime.max_output_bytes),
            ));
        }
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::new(
                "ai_sdk_provider_failed",
                &self.catalog.provider.name,
                Some(call.action),
                format!("AI SDK provider failed: {}", redact_public(&stderr)),
                "Fix the TypeScript provider handler and retry.",
            ));
        }

        let value = serde_json::from_slice(&output.stdout).map_err(|error| {
            ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "ai_sdk_invalid_json_output",
                error.to_string(),
            )
        })?;
        Ok(ProviderOutput::json(value))
    }
}

impl AiSdkProvider {
    fn tool(&self, call: &ProviderCall) -> Result<&ProviderTool, ProviderError> {
        self.catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_ai_sdk_action",
                    format!("AI SDK provider has no action `{}`", call.action),
                )
            })
    }
}

struct SidecarRuntime {
    command: String,
    env: Vec<(String, String)>,
    timeout_ms: u64,
    max_input_bytes: usize,
    max_output_bytes: usize,
}

impl SidecarRuntime {
    fn from_tool(
        catalog: &ProviderCatalog,
        tool: &ProviderTool,
        call: &ProviderCall,
    ) -> Result<Self, ProviderError> {
        let meta = tool.meta.get("ai_sdk").or_else(|| tool.meta.get("sidecar"));
        let command = meta
            .and_then(|value| value.get("command"))
            .and_then(Value::as_str)
            .unwrap_or("node")
            .to_owned();
        let timeout_ms = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.timeout_ms)
            .or_else(|| {
                meta.and_then(|value| value.get("timeout_ms"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(10_000);
        let max_input_bytes = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.max_input_bytes)
            .unwrap_or(64 * 1024);
        let max_output_bytes = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.max_response_bytes)
            .unwrap_or(256 * 1024);
        Ok(Self {
            command,
            env: collect_provider_env(&catalog.env, &tool.env, call)?,
            timeout_ms,
            max_input_bytes,
            max_output_bytes,
        })
    }
}

struct SidecarWrapper {
    source: String,
}

impl SidecarWrapper {
    fn new(provider_path: &std::path::Path) -> std::io::Result<Self> {
        let canonical = provider_path.canonicalize()?;
        let module_path = canonical.display().to_string();
        let source = format!(
            r#"
import {{ readFileSync }} from "node:fs";
const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const input = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{{}}");
let providerSource = readFileSync({module_path:?}, "utf8");
providerSource = removeDefaultManifest(providerSource);
const module = await import("data:text/javascript;base64," + Buffer.from(providerSource).toString("base64"));
const handler = module.call || module.default?.call;
if (typeof handler !== "function") {{
  throw new Error("TypeScript provider must export async function call(input)");
}}
const result = await handler(input);
process.stdout.write(JSON.stringify(result ?? null));

function removeDefaultManifest(source) {{
  const marker = "export default";
  const start = source.indexOf(marker);
  if (start < 0) return source;
  const open = source.indexOf("{{", start + marker.length);
  if (open < 0) return source;
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let i = open; i < source.length; i++) {{
    const ch = source[i];
    if (inString) {{
      if (escaped) escaped = false;
      else if (ch === "\\\\") escaped = true;
      else if (ch === "\"") inString = false;
      continue;
    }}
    if (ch === "\"") inString = true;
    else if (ch === "{{") depth++;
    else if (ch === "}}") {{
      depth--;
      if (depth === 0) {{
        let end = i + 1;
        while (source[end] && /\\s/.test(source[end])) end++;
        if (source[end] === ";") end++;
        return source.slice(0, start) + source.slice(end);
      }}
    }}
  }}
  return source;
}}
"#
        );
        Ok(Self { source })
    }

    fn source(&self) -> &str {
        &self.source
    }
}
