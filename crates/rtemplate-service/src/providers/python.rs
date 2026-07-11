use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio as StdStdio},
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use rtemplate_contracts::providers::{EnvRequirement, ProviderCatalog, ProviderTool};
use serde_json::{json, Value};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    time::{timeout, Instant as TokioInstant},
};

use crate::{
    provider_errors::{redact_public, ProviderError},
    provider_registry::{Provider, ProviderCall, ProviderOutput},
};

const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_MAX_INPUT_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct PythonProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
}

impl PythonProvider {
    pub fn new(path: PathBuf, catalog: ProviderCatalog) -> Self {
        Self { path, catalog }
    }

    pub fn arc(path: PathBuf, catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::new(path, catalog))
    }
}

#[async_trait]
impl Provider for PythonProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let tool = self.tool(&call)?;
        let runtime = PythonRuntime::from_tool(&self.catalog, tool, &call)?;
        let input = serde_json::to_vec(&json!({
            "mode": "call",
            "path": self.path,
            "action": call.action,
            "params": call.params,
        }))
        .map_err(|error| ProviderError::execution(&self.catalog.provider.name, "", error))?;

        if input.len() > runtime.max_input_bytes {
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "python_input_too_large",
                format!(
                    "Python provider input exceeds {} bytes",
                    runtime.max_input_bytes
                ),
            ));
        }

        let started = TokioInstant::now();
        let mut child = Command::new(&runtime.command)
            .args(["-c", PYTHON_BRIDGE])
            .envs(runtime.env)
            .stdin(StdStdio::piped())
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped())
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
                "python_provider_timeout",
                &self.catalog.provider.name,
                Some(call.action.clone()),
                format!("Python provider exceeded {}ms timeout", runtime.timeout_ms),
                "Increase tool.limits.timeout_ms or fix the Python provider handler.",
            )
        })?
        .map_err(|error| {
            ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
        })?;

        tracing::debug!(
            provider = %self.catalog.provider.name,
            action = %call.action,
            elapsed_ms = started.elapsed().as_millis(),
            "Python provider sidecar completed"
        );

        if output.stdout.len() > runtime.max_output_bytes {
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "python_output_too_large",
                format!(
                    "Python provider output exceeds {} bytes",
                    runtime.max_output_bytes
                ),
            ));
        }
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::new(
                "python_provider_failed",
                &self.catalog.provider.name,
                Some(call.action),
                format!("Python provider failed: {}", redact_public(&stderr)),
                "Fix the Python provider handler and retry.",
            ));
        }

        let value = serde_json::from_slice(&output.stdout).map_err(|error| {
            ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "python_invalid_json_output",
                error.to_string(),
            )
        })?;
        Ok(ProviderOutput::json(value))
    }
}

impl PythonProvider {
    fn tool(&self, call: &ProviderCall) -> Result<&ProviderTool, ProviderError> {
        self.catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_python_action",
                    format!("Python provider has no action `{}`", call.action),
                )
            })
    }
}

pub fn load_python_catalog(path: &Path) -> Result<ProviderCatalog, String> {
    let runtime = PythonRuntime::for_catalog();
    let input = serde_json::to_vec(&json!({
        "mode": "catalog",
        "path": path,
    }))
    .map_err(|error| error.to_string())?;
    let output = run_catalog_sidecar(&runtime, &input)?;
    serde_json::from_slice(&output).map_err(|error| error.to_string())
}

struct PythonRuntime {
    command: String,
    env: Vec<(String, String)>,
    timeout_ms: u64,
    max_input_bytes: usize,
    max_output_bytes: usize,
}

impl PythonRuntime {
    fn for_catalog() -> Self {
        Self {
            command: std::env::var("RTEMPLATE_PYTHON_COMMAND")
                .unwrap_or_else(|_| "python3".to_owned()),
            env: Vec::new(),
            timeout_ms: std::env::var("RTEMPLATE_PYTHON_CATALOG_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_TIMEOUT_MS),
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }

    fn from_tool(
        catalog: &ProviderCatalog,
        tool: &ProviderTool,
        call: &ProviderCall,
    ) -> Result<Self, ProviderError> {
        let meta = tool
            .meta
            .get("python")
            .or_else(|| catalog.meta.get("python"));
        let command = meta
            .and_then(|value| value.get("command"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| std::env::var("RTEMPLATE_PYTHON_COMMAND").ok())
            .unwrap_or_else(|| "python3".to_owned());
        let timeout_ms = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.timeout_ms)
            .or_else(|| {
                meta.and_then(|value| value.get("timeout_ms"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let max_input_bytes = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.max_input_bytes)
            .unwrap_or(DEFAULT_MAX_INPUT_BYTES);
        let max_output_bytes = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.max_response_bytes)
            .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
        Ok(Self {
            command,
            env: collect_env(&catalog.env, &tool.env, call)?,
            timeout_ms,
            max_input_bytes,
            max_output_bytes,
        })
    }
}

fn collect_env(
    provider_requirements: &[EnvRequirement],
    tool_requirements: &[EnvRequirement],
    call: &ProviderCall,
) -> Result<Vec<(String, String)>, ProviderError> {
    let mut env = Vec::new();
    for requirement in provider_requirements.iter().chain(tool_requirements) {
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

fn run_catalog_sidecar(runtime: &PythonRuntime, input: &[u8]) -> Result<Vec<u8>, String> {
    let mut child = StdCommand::new(&runtime.command)
        .args(["-c", PYTHON_BRIDGE])
        .stdin(StdStdio::piped())
        .stdout(StdStdio::piped())
        .stderr(StdStdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input).map_err(|error| error.to_string())?;
    }
    let deadline = Instant::now() + Duration::from_millis(runtime.timeout_ms);
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            let mut stdout = Vec::new();
            if let Some(mut pipe) = child.stdout.take() {
                pipe.read_to_end(&mut stdout)
                    .map_err(|error| error.to_string())?;
            }
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr)
                    .map_err(|error| error.to_string())?;
            }
            if stdout.len() > runtime.max_output_bytes {
                return Err(format!(
                    "Python provider catalog exceeds {} bytes",
                    runtime.max_output_bytes
                ));
            }
            if !status.success() {
                return Err(format!(
                    "Python provider catalog failed: {}",
                    redact_public(&stderr)
                ));
            }
            return Ok(stdout);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err(format!(
                "Python provider catalog exceeded {}ms timeout",
                runtime.timeout_ms
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

const PYTHON_BRIDGE: &str = r#"
import asyncio
import dataclasses
import importlib.util
import inspect
import json
import re
import sys
from pathlib import Path


def load_module(path):
    path = Path(path).resolve()
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location("_rtemplate_python_provider", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import provider file {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def provider_config(module):
    value = getattr(module, "PROVIDER", None)
    if isinstance(value, dict):
        return dict(value)
    return {}


def slug(value):
    value = re.sub(r"[^a-zA-Z0-9]+", "-", value).strip("-").lower()
    return value or "python-provider"


def expand_tools(module):
    raw = getattr(module, "TOOLS", None)
    if raw is None:
        raw = getattr(module, "tools", None)
    if raw is None:
        raw = []
    expanded = []
    for item in raw:
        to_tool_list = getattr(item, "to_tool_list", None)
        if callable(to_tool_list):
            expanded.extend(to_tool_list())
        else:
            expanded.append(item)
    return expanded


def detect_kind(module, tools, config):
    kind = config.get("kind") or getattr(module, "PROVIDER_KIND", None)
    if kind:
        return kind
    for tool in tools:
        metadata = getattr(tool, "metadata", None)
        if metadata is not None and (
            hasattr(metadata, "fn_schema") or hasattr(metadata, "get_parameters_dict")
        ):
            return "llamaindex"
        if hasattr(tool, "args_schema") or hasattr(tool, "args") or hasattr(tool, "invoke"):
            return "langchain"
    raise RuntimeError("Python provider must expose PROVIDER['kind'] or detectable tools")


def jsonable(value):
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, list) or isinstance(value, tuple):
        return [jsonable(item) for item in value]
    if isinstance(value, dict):
        return {str(key): jsonable(item) for key, item in value.items()}
    if dataclasses.is_dataclass(value):
        return jsonable(dataclasses.asdict(value))
    model_dump = getattr(value, "model_dump", None)
    if callable(model_dump):
        return jsonable(model_dump())
    dict_method = getattr(value, "dict", None)
    if callable(dict_method):
        try:
            return jsonable(dict_method())
        except TypeError:
            pass
    if hasattr(value, "content"):
        return {"content": jsonable(getattr(value, "content"))}
    return str(value)


def model_schema(value):
    if value is None:
        return None
    if isinstance(value, dict):
        return jsonable(value)
    for method_name in ("model_json_schema", "schema"):
        method = getattr(value, method_name, None)
        if callable(method):
            return jsonable(method())
    return None


def object_schema(schema):
    schema = schema or {}
    if schema.get("type") == "object":
        schema.setdefault("additionalProperties", False)
        return schema
    if "properties" in schema:
        schema["type"] = "object"
        schema.setdefault("additionalProperties", False)
        return schema
    return {"type": "object", "additionalProperties": False, "properties": {}}


def langchain_schema(tool):
    schema = model_schema(getattr(tool, "args_schema", None))
    if schema:
        return object_schema(schema)
    args = getattr(tool, "args", None)
    if isinstance(args, dict):
        return object_schema({"type": "object", "properties": jsonable(args)})
    return object_schema(None)


def llamaindex_schema(tool):
    metadata = getattr(tool, "metadata", None)
    schema = model_schema(getattr(metadata, "fn_schema", None))
    if schema:
        return object_schema(schema)
    get_parameters = getattr(metadata, "get_parameters_dict", None)
    if callable(get_parameters):
        return object_schema(jsonable(get_parameters()))
    return object_schema(None)


def tool_name(tool, kind):
    if kind == "llamaindex":
        metadata = getattr(tool, "metadata", None)
        value = getattr(metadata, "name", None)
        if value:
            return value
    return getattr(tool, "name", None) or getattr(tool, "__name__", None)


def tool_description(tool, kind):
    if kind == "llamaindex":
        metadata = getattr(tool, "metadata", None)
        value = getattr(metadata, "description", None)
        if value:
            return value
    return getattr(tool, "description", None) or inspect.getdoc(tool) or "Python provider tool."


def tool_schema(tool, kind):
    if kind == "llamaindex":
        return llamaindex_schema(tool)
    return langchain_schema(tool)


def catalog(path):
    module = load_module(path)
    config = provider_config(module)
    tools = expand_tools(module)
    kind = detect_kind(module, tools, config)
    if kind not in ("langchain", "llamaindex"):
        raise RuntimeError(f"unsupported Python provider kind {kind!r}")
    provider = {
        "name": config.get("name") or slug(Path(path).stem),
        "kind": kind,
    }
    for key in ("title", "description", "homepage", "source", "version", "enabled"):
        if key in config:
            provider[key] = config[key]
    output = {
        "schema_version": 1,
        "provider": provider,
        "tools": [],
        "meta": config.get("meta") or {},
    }
    for tool in tools:
        name = tool_name(tool, kind)
        if not name:
            raise RuntimeError("Python provider tool is missing a name")
        output["tools"].append({
            "name": name,
            "description": tool_description(tool, kind),
            "input_schema": tool_schema(tool, kind),
            "meta": {"python": {"adapter": kind}},
        })
    return output


def resolve_tool(module, action):
    config = provider_config(module)
    tools = expand_tools(module)
    kind = detect_kind(module, tools, config)
    for tool in tools:
        if tool_name(tool, kind) == action:
            return kind, tool
    raise RuntimeError(f"unknown Python provider action {action!r}")


async def maybe_await(value):
    if inspect.isawaitable(value):
        return await value
    return value


async def call_langchain(tool, params):
    ainvoke = getattr(tool, "ainvoke", None)
    if callable(ainvoke):
        return await maybe_await(ainvoke(params))
    invoke = getattr(tool, "invoke", None)
    if callable(invoke):
        return await maybe_await(invoke(params))
    if callable(tool):
        return await maybe_await(tool(**params))
    raise RuntimeError("LangChain tool is not callable")


async def call_llamaindex(tool, params):
    acall = getattr(tool, "acall", None)
    if callable(acall):
        return await maybe_await(acall(**params))
    call = getattr(tool, "call", None)
    if callable(call):
        return await maybe_await(call(**params))
    if callable(tool):
        return await maybe_await(tool(**params))
    raise RuntimeError("LlamaIndex tool is not callable")


async def execute(path, action, params):
    module = load_module(path)
    kind, tool = resolve_tool(module, action)
    if kind == "llamaindex":
        return await call_llamaindex(tool, params)
    return await call_langchain(tool, params)


async def main():
    payload = json.loads(sys.stdin.buffer.read().decode("utf-8") or "{}")
    mode = payload.get("mode")
    if mode == "catalog":
        result = catalog(payload["path"])
    elif mode == "call":
        result = await execute(payload["path"], payload["action"], payload.get("params") or {})
    else:
        raise RuntimeError(f"unknown Python bridge mode {mode!r}")
    sys.stdout.write(json.dumps(jsonable(result), separators=(",", ":")))


asyncio.run(main())
"#;
