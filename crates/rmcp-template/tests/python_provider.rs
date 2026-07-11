use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use rtemplate_contracts::config::ExampleConfig;
use rtemplate_service::{
    dynamic_provider_registry_from_dir, provider_registry::ProviderAuthMode,
    provider_registry::ProviderCall, provider_registry::ProviderPrincipal,
    provider_registry::ProviderRequestLimits, provider_registry::ProviderSurface, ExampleClient,
    ExampleService,
};
use serde_json::json;

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

async fn dispatch(
    registry: &rtemplate_service::ProviderRegistry,
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

fn service() -> anyhow::Result<ExampleService> {
    let client = ExampleClient::new(&ExampleConfig {
        api_url: String::new(),
        api_key: "test".to_owned(),
    })?;
    Ok(ExampleService::new(client))
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
