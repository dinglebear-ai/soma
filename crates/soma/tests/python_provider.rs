use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use serde_json::json;
use soma_contracts::config::SomaConfig;
use soma_service::{
    dynamic_provider_registry_from_dir, provider_registry::ProviderAuthMode,
    provider_registry::ProviderCall, provider_registry::ProviderPrincipal,
    provider_registry::ProviderRequestLimits, provider_registry::ProviderSurface, SomaClient,
    SomaService,
};

#[tokio::test]
async fn langchain_provider_executes_hot_dropped_python_tool() -> anyhow::Result<()> {
    let temp = test_dir("langchain")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("live_langchain.py"),
        r#"
PROVIDER = {"name": "live-langchain", "kind": "langchain"}

class WeatherTool:
    name = "live_langchain_weather"
    description = "Get weather from a LangChain-style tool."
    args = {
        "city": {"type": "string", "description": "City name"}
    }

    def invoke(self, params):
        return {
            "ok": True,
            "runtime": "langchain",
            "city": params["city"],
        }

TOOLS = [WeatherTool()]
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "live-langchain")
        .expect("langchain provider catalog");
    assert_eq!(catalog.provider.kind.as_str(), "langchain");
    assert_eq!(catalog.tools[0].name, "live_langchain_weather");
    assert_eq!(
        catalog.tools[0].input_schema["properties"]["city"]["type"],
        "string"
    );

    let output = dispatch(&registry, "live_langchain_weather", json!({"city": "Oslo"})).await?;

    assert_eq!(output["ok"], true);
    assert_eq!(output["runtime"], "langchain");
    assert_eq!(output["city"], "Oslo");
    Ok(())
}

#[tokio::test]
async fn llamaindex_provider_executes_hot_dropped_python_tool() -> anyhow::Result<()> {
    let temp = test_dir("llamaindex")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("live_llamaindex.py"),
        r#"
PROVIDER = {"name": "live-llamaindex", "kind": "llamaindex"}

class Metadata:
    name = "live_llamaindex_lookup"
    description = "Look up data from a LlamaIndex-style tool."
    fn_schema = {
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "query": {"type": "string", "description": "Lookup query"}
        },
        "required": ["query"],
    }

class LookupTool:
    metadata = Metadata()

    def call(self, **kwargs):
        return {
            "ok": True,
            "runtime": "llamaindex",
            "query": kwargs["query"],
        }

TOOLS = [LookupTool()]
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "live-llamaindex")
        .expect("llamaindex provider catalog");
    assert_eq!(catalog.provider.kind.as_str(), "llamaindex");
    assert_eq!(catalog.tools[0].name, "live_llamaindex_lookup");
    assert_eq!(
        catalog.tools[0].input_schema["properties"]["query"]["type"],
        "string"
    );

    let output = dispatch(
        &registry,
        "live_llamaindex_lookup",
        json!({"query": "status"}),
    )
    .await?;

    assert_eq!(output["ok"], true);
    assert_eq!(output["runtime"], "llamaindex");
    assert_eq!(output["query"], "status");
    Ok(())
}

#[tokio::test]
async fn real_langchain_provider_smoke_when_available() -> anyhow::Result<()> {
    if !python_module_available("langchain_core.tools") {
        eprintln!("skipping real LangChain smoke: langchain_core.tools is not installed");
        return Ok(());
    }

    let temp = test_dir("real-langchain")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("real_langchain.py"),
        r#"
from langchain_core.tools import tool

PROVIDER = {"name": "real-langchain", "kind": "langchain"}

@tool
def real_langchain_multiply(a: int, b: int) -> dict:
    """Multiply two integers with a real LangChain tool."""
    return {"product": a * b}

TOOLS = [real_langchain_multiply]
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "real-langchain")
        .expect("real LangChain catalog");
    let tool = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "real_langchain_multiply")
        .expect("real LangChain tool");
    assert_eq!(tool.input_schema["properties"]["a"]["type"], "integer");
    assert_eq!(tool.input_schema["properties"]["b"]["type"], "integer");

    let output = dispatch(
        &registry,
        "real_langchain_multiply",
        json!({"a": 6, "b": 7}),
    )
    .await?;
    assert_eq!(output["product"], 42);
    Ok(())
}

#[tokio::test]
async fn real_llamaindex_provider_smoke_when_available() -> anyhow::Result<()> {
    if !python_module_available("llama_index.core.tools") {
        eprintln!("skipping real LlamaIndex smoke: llama_index.core.tools is not installed");
        return Ok(());
    }

    let temp = test_dir("real-llamaindex")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("real_llamaindex.py"),
        r#"
from llama_index.core.tools import FunctionTool

PROVIDER = {"name": "real-llamaindex", "kind": "llamaindex"}

def real_llamaindex_add(a: int, b: int) -> dict:
    """Add two integers with a real LlamaIndex FunctionTool."""
    return {"sum": a + b}

TOOLS = [FunctionTool.from_defaults(real_llamaindex_add, name="real_llamaindex_add")]
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "real-llamaindex")
        .expect("real LlamaIndex catalog");
    let tool = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "real_llamaindex_add")
        .expect("real LlamaIndex tool");
    assert_eq!(tool.input_schema["properties"]["a"]["type"], "integer");
    assert_eq!(tool.input_schema["properties"]["b"]["type"], "integer");

    let output = dispatch(&registry, "real_llamaindex_add", json!({"a": 2, "b": 3})).await?;
    assert!(
        output.to_string().contains('5'),
        "real LlamaIndex output should include the computed sum: {output}"
    );
    Ok(())
}

#[tokio::test]
async fn plain_python_provider_discovers_and_executes_public_functions() -> anyhow::Result<()> {
    let temp = test_dir("plain")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("plain_math.py"),
        r#"
PROVIDER = {"name": "plain-python", "kind": "python"}

def add(a: int, b: int) -> int:
    """Add two integers."""
    return a + b

async def reflect_message(message: str) -> dict:
    """Echo a message."""
    return {"message": message}

def _private() -> str:
    return "hidden"
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "plain-python")
        .expect("plain python provider catalog");
    assert_eq!(catalog.provider.kind.as_str(), "python");
    assert!(catalog.tools.iter().any(|tool| tool.name == "add"));
    assert!(catalog
        .tools
        .iter()
        .any(|tool| tool.name == "reflect_message"));
    assert!(!catalog.tools.iter().any(|tool| tool.name == "_private"));

    let add = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "add")
        .expect("add tool");
    assert_eq!(add.description, "Add two integers.");
    assert_eq!(add.input_schema["properties"]["a"]["type"], "integer");
    assert_eq!(add.input_schema["properties"]["b"]["type"], "integer");
    assert_eq!(add.input_schema["required"], json!(["a", "b"]));

    let output = dispatch(&registry, "add", json!({"a": 2, "b": 3})).await?;
    assert_eq!(output, json!(5));

    let output = dispatch(&registry, "reflect_message", json!({"message": "hi"})).await?;
    assert_eq!(output, json!({"message": "hi"}));
    Ok(())
}

#[tokio::test]
async fn plain_python_provider_cli_dispatches_generated_tool() -> anyhow::Result<()> {
    let temp = test_dir("plain-cli")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("plain_cli.py"),
        r#"
PROVIDER = {"name": "plain-cli-python", "kind": "python"}

def shout(message: str) -> dict:
    """Uppercase a message."""
    return {"message": message.upper()}
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_soma"))
        .arg("shout")
        .arg("--json")
        .arg(r#"{"message":"hello"}"#)
        .current_dir(&temp)
        .env("SOMA_HOME", &temp)
        .env("SOMA_API_URL", "")
        .env("SOMA_API_KEY", "")
        .env_remove("SOMA_MCP_TOKEN")
        .env_remove("SOMA_PROVIDER_DIR")
        .output()?;

    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value, json!({"message": "HELLO"}));
    Ok(())
}

#[tokio::test]
async fn python_provider_ignores_provider_stdout_noise() -> anyhow::Result<()> {
    let temp = test_dir("stdout-noise")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("noisy.py"),
        r#"
print("import log line")

PROVIDER = {"name": "noisy-python", "kind": "python"}

def noisy(value: str) -> dict:
    """Return a value while printing to stdout."""
    print("call log line")
    return {"value": value}
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let output = dispatch(&registry, "noisy", json!({"value": "kept"})).await?;

    assert_eq!(output, json!({"value": "kept"}));
    Ok(())
}

#[tokio::test]
async fn python_provider_respects_explicit_empty_tools_list() -> anyhow::Result<()> {
    let temp = test_dir("empty-tools")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("empty_tools.py"),
        r#"
PROVIDER = {"name": "empty-tools-python", "kind": "python"}
TOOLS = []

def accidental() -> str:
    """This function should not be exposed when TOOLS is explicitly empty."""
    return "leaked"
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "empty-tools-python")
        .expect("empty tools provider catalog");
    assert!(
        catalog.tools.is_empty(),
        "explicit TOOLS = [] should expose no tools"
    );

    let result = dispatch(&registry, "accidental", json!({})).await;
    assert!(
        result.is_err(),
        "empty-tools provider must not dispatch accidental public functions"
    );
    Ok(())
}

#[tokio::test]
async fn python_provider_rejects_positional_only_plain_functions() -> anyhow::Result<()> {
    let temp = test_dir("positional-only")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("positional_only.py"),
        r#"
PROVIDER = {"name": "positional-only-python", "kind": "python"}

def positional_only(value, /) -> str:
    """Cannot be called from named JSON params."""
    return value
"#,
    )?;

    let error = match dynamic_provider_registry_from_dir(service()?, &providers) {
        Ok(_) => anyhow::bail!("positional-only Python tool should not be cataloged"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("positional-only"),
        "error should explain the unsupported positional-only parameter: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn python_provider_rejects_oversized_stderr_output() -> anyhow::Result<()> {
    let temp = test_dir("stderr-cap")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("stderr_cap.py"),
        r#"
PROVIDER = {"name": "stderr-cap-python", "kind": "python"}

def noisy_failure() -> None:
    """Write too much stderr before failing."""
    import sys

    sys.stderr.write("x" * (300 * 1024))
    raise RuntimeError("boom")
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let result = dispatch(&registry, "noisy_failure", json!({})).await;
    let error = result.expect_err("oversized stderr should fail");

    assert!(
        error.to_string().contains("python_output_too_large"),
        "stderr larger than the response cap should fail with output-too-large: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn python_provider_rejects_unresolved_annotations() -> anyhow::Result<()> {
    let temp = test_dir("annotation-error")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("annotation_error.py"),
        r#"
from __future__ import annotations

PROVIDER = {"name": "annotation-error-python", "kind": "python"}

def lookup(client: MissingClient) -> dict:
    """Use an annotation that cannot be resolved."""
    return {"client": str(client)}
"#,
    )?;

    let error = match dynamic_provider_registry_from_dir(service()?, &providers) {
        Ok(_) => anyhow::bail!("unresolved Python annotations should fail catalog loading"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("lookup") && error.to_string().contains("MissingClient"),
        "error should name the tool and unresolved annotation: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn python_provider_rejects_unserializable_outputs() -> anyhow::Result<()> {
    let temp = test_dir("unserializable-output")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("unserializable.py"),
        r#"
PROVIDER = {"name": "unserializable-python", "kind": "python"}

class Custom:
    pass

def make_custom() -> object:
    """Return a custom object that is not JSON-compatible."""
    return Custom()
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let result = dispatch(&registry, "make_custom", json!({})).await;
    let error = result.expect_err("custom object output should fail");

    assert!(
        error
            .to_string()
            .contains("python_provider_unserializable_output"),
        "unsupported output types should be surfaced as serialization errors: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn python_provider_kills_timed_out_tool_process() -> anyhow::Result<()> {
    let temp = test_dir("timeout")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("slow.py"),
        r#"
PROVIDER = {
    "name": "timeout-python",
    "kind": "python",
    "meta": {"python": {"timeout_ms": 100}},
}

def slow(marker: str) -> dict:
    """Sleep long enough to exceed the configured timeout."""
    import pathlib
    import time

    time.sleep(0.5)
    pathlib.Path(marker).write_text("still running")
    return {"done": True}
"#,
    )?;
    let marker = temp.join("timed-out-sidecar.txt");

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let result = dispatch(&registry, "slow", json!({"marker": marker})).await;

    assert!(result.is_err(), "slow provider call should time out");
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        !marker.exists(),
        "timed-out Python sidecar should not continue running after timeout"
    );
    Ok(())
}

#[tokio::test]
async fn python_provider_passes_only_declared_environment() -> anyhow::Result<()> {
    let temp = test_dir("env-boundary")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("env_reader.py"),
        r#"
import os

PROVIDER = {
    "name": "env-python",
    "kind": "python",
    "env": [
        {
            "name": "PYTHON_PROVIDER_DECLARED_SECRET",
            "server_prefixed": False,
            "required": True,
            "sensitive": True,
            "default": "allowed",
        }
    ],
}

def read_env() -> dict:
    """Read declared and undeclared environment values."""
    return {
        "declared": os.environ.get("PYTHON_PROVIDER_DECLARED_SECRET"),
        "undeclared_path": os.environ.get("PATH", "missing"),
    }
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let result = dispatch(&registry, "read_env", json!({})).await?;
    assert_eq!(
        result,
        json!({"declared": "allowed", "undeclared_path": "missing"})
    );
    Ok(())
}

#[tokio::test]
async fn json_manifest_cannot_claim_python_provider_kind() -> anyhow::Result<()> {
    let temp = test_dir("json-python-kind")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("bad-python.json"),
        r#"
{
  "schema_version": 1,
  "provider": {
    "name": "bad-python-json",
    "kind": "python"
  },
  "tools": [
    {
      "name": "bad_python_json",
      "description": "A Python provider kind declared from JSON.",
      "input_schema": {
        "type": "object",
        "additionalProperties": false,
        "properties": {}
      }
    }
  ]
}
"#,
    )?;

    let error = match dynamic_provider_registry_from_dir(service()?, &providers) {
        Ok(_) => anyhow::bail!("JSON manifests must not claim Python provider kinds"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("requires a .py file"));
    Ok(())
}

#[tokio::test]
async fn json_manifests_cannot_claim_executable_provider_kinds() -> anyhow::Result<()> {
    for (kind, filename, expected) in [
        ("ai-sdk", "bad-ai-sdk.json", ".ts"),
        ("wasm", "bad-wasm.json", ".wasm"),
    ] {
        let temp = test_dir(kind)?;
        let providers = temp.join("providers");
        fs::create_dir(&providers)?;
        fs::write(
            providers.join(filename),
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "provider": {
                    "name": format!("bad-{kind}"),
                    "kind": kind
                },
                "tools": [{
                    "name": format!("bad_{}", kind.replace('-', "_")),
                    "description": "Executable provider kind declared from JSON.",
                    "input_schema": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {}
                    }
                }]
            }))?,
        )?;

        let error = match dynamic_provider_registry_from_dir(service()?, &providers) {
            Ok(_) => anyhow::bail!("JSON manifests must not claim {kind} provider kind"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains(expected),
            "{kind} JSON manifest should require {expected}: {error}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn python_provider_rejects_framework_tool_names_outside_manifest_contract(
) -> anyhow::Result<()> {
    let temp = test_dir("invalid-tool-name")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("bad_name.py"),
        r#"
PROVIDER = {"name": "bad-name-python", "kind": "langchain"}

class BadNameTool:
    name = "Bad Tool"
    description = "Invalid public tool name."
    args = {}

    def invoke(self, params):
        return {"ok": True}

TOOLS = [BadNameTool()]
"#,
    )?;

    let error = match dynamic_provider_registry_from_dir(service()?, &providers) {
        Ok(_) => anyhow::bail!("invalid generated Python tool names should fail validation"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("json_schema_failed"),
        "generated catalog should be checked against the manifest schema: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn python_provider_registers_module_during_import() -> anyhow::Result<()> {
    let temp = test_dir("sys-modules")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("module_identity.py"),
        r#"
import sys

PROVIDER = {"name": "module-identity", "kind": "python"}

MODULE_PRESENT_DURING_IMPORT = __name__ in sys.modules

def module_present() -> dict:
    """Return whether the provider module was registered during import."""
    return {"present": MODULE_PRESENT_DURING_IMPORT}
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let output = dispatch(&registry, "module_present", json!({})).await?;

    assert_eq!(output, json!({"present": true}));
    Ok(())
}

#[tokio::test]
async fn python_provider_handles_union_annotations_without_union_type() -> anyhow::Result<()> {
    let temp = test_dir("union-type")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("union_type.py"),
        r#"
from __future__ import annotations

PROVIDER = {"name": "union-python", "kind": "python"}

def maybe(value: str | None = None) -> dict:
    """Return an optional value."""
    return {"value": value}
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "union-python")
        .expect("union python provider catalog");
    let maybe = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "maybe")
        .expect("maybe tool");

    assert_eq!(
        maybe.input_schema["properties"]["value"]["anyOf"],
        json!([{"type": "string"}, {"type": "null"}])
    );
    assert!(maybe.input_schema.get("required").is_none());

    let output = dispatch(&registry, "maybe", json!({"value": null})).await?;
    assert_eq!(output, json!({"value": null}));
    Ok(())
}

#[tokio::test]
async fn python_provider_schema_does_not_require_types_union_type() -> anyhow::Result<()> {
    let temp = test_dir("missing-union-type")?;
    let providers = temp.join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("missing_union_type.py"),
        r#"
import types
import typing

if hasattr(types, "UnionType"):
    delattr(types, "UnionType")

PROVIDER = {"name": "missing-union-type", "kind": "python"}

def maybe(value: typing.Optional[str] = None) -> dict:
    """Return an optional value."""
    return {"value": value}
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let snapshot = registry.snapshot();
    let catalog = snapshot
        .catalogs
        .iter()
        .find(|catalog| catalog.provider.name == "missing-union-type")
        .expect("missing-union-type catalog");
    let maybe = catalog
        .tools
        .iter()
        .find(|tool| tool.name == "maybe")
        .expect("maybe tool");

    assert_eq!(
        maybe.input_schema["properties"]["value"]["anyOf"],
        json!([{"type": "string"}, {"type": "null"}])
    );
    let output = dispatch(&registry, "maybe", json!({"value": null})).await?;
    assert_eq!(output, json!({"value": null}));
    Ok(())
}

async fn dispatch(
    registry: &soma_service::ProviderRegistry,
    action: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let output = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: action.to_owned(),
            params,
            principal: ProviderPrincipal::loopback_dev(),
            auth_mode: ProviderAuthMode::LoopbackDev,
            surface: ProviderSurface::Mcp,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await?;
    Ok(output.value)
}

fn service() -> anyhow::Result<SomaService> {
    let client = SomaClient::new(&SomaConfig {
        api_url: String::new(),
        api_key: "test".to_owned(),
    })?;
    Ok(SomaService::new(client))
}

fn python_module_available(module: &str) -> bool {
    let code = format!(
        "import importlib.util, sys\ntry:\n    found = importlib.util.find_spec({module:?}) is not None\nexcept ModuleNotFoundError:\n    found = False\nsys.exit(0 if found else 1)"
    );
    Command::new("python3")
        .args(["-c", &code])
        .status()
        .is_ok_and(|status| status.success())
}

fn test_dir(name: &str) -> anyhow::Result<PathBuf> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/python-provider-tests")
        .join(format!("{name}-{id}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}
