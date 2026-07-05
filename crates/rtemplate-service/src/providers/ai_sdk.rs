use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use rtemplate_contracts::providers::{EnvRequirement, ProviderCatalog, ProviderTool};
use serde_json::{json, Value};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    time::{timeout, Instant},
};

use crate::{
    provider_errors::{redact_public, ProviderError},
    provider_registry::{Provider, ProviderCall, ProviderOutput},
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
        let runtime = SidecarRuntime::from_tool(tool, &call)?;
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
        let mut child = Command::new(&runtime.command)
            .args(["--input-type=module", "--eval", wrapper.source()])
            .env_clear()
            .envs(runtime.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&input).await.map_err(|error| {
                ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
            })?;
        }

        let output = timeout(
            Duration::from_millis(runtime.timeout_ms),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| {
            ProviderError::new(
                "ai_sdk_provider_timeout",
                &self.catalog.provider.name,
                Some(call.action.clone()),
                format!("AI SDK provider exceeded {}ms timeout", runtime.timeout_ms),
                "Increase tool.limits.timeout_ms or fix the provider handler.",
            )
        })?
        .map_err(|error| {
            ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
        })?;

        tracing::debug!(
            provider = %self.catalog.provider.name,
            action = %call.action,
            elapsed_ms = started.elapsed().as_millis(),
            "AI SDK provider sidecar completed"
        );

        if output.stdout.len() > runtime.max_output_bytes {
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "ai_sdk_output_too_large",
                format!("AI SDK output exceeds {} bytes", runtime.max_output_bytes),
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
    fn from_tool(tool: &ProviderTool, call: &ProviderCall) -> Result<Self, ProviderError> {
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
            env: collect_env(&tool.env, call)?,
            timeout_ms,
            max_input_bytes,
            max_output_bytes,
        })
    }
}

fn collect_env(
    requirements: &[EnvRequirement],
    call: &ProviderCall,
) -> Result<Vec<(String, String)>, ProviderError> {
    let mut env = Vec::new();
    for requirement in requirements {
        let name = requirement.runtime_name("RTEMPLATE");
        let value = std::env::var(&name)
            .ok()
            .or_else(|| {
                requirement
                    .allow_unprefixed
                    .then(|| std::env::var(&requirement.name).ok())
                    .flatten()
            })
            .or_else(|| {
                requirement
                    .default
                    .as_ref()
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });
        match value {
            Some(value) => env.push((name, value)),
            None if requirement.required => {
                return Err(ProviderError::validation(
                    &call.provider,
                    &call.action,
                    "missing_provider_env",
                    format!("missing required provider env `{name}`"),
                ));
            }
            None => {}
        }
    }
    Ok(env)
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
