use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use rtemplate_contracts::providers::{
    CapabilityGrant, HostCapabilities, NetworkCapability, ProviderCatalog, ProviderIdentity,
    ProviderKind, ProviderManifest, ProviderTool, RestOverlay,
};
use rtemplate_service::capabilities::CapabilityBroker;
use rtemplate_service::provider_registry::{
    Provider, ProviderAuthMode, ProviderCall, ProviderOutput, ProviderPrincipal, ProviderRegistry,
    ProviderRequestLimits, ProviderSurface,
};
use rtemplate_service::ProviderError;
use serde_json::json;
use tokio::sync::Notify;

fn tool(name: &str) -> ProviderTool {
    ProviderTool {
        name: name.to_owned(),
        description: format!("{name} tool"),
        title: None,
        input_schema: json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "message": { "type": "string" }
            }
        }),
        output_schema: None,
        scope: Some("example:read".to_owned()),
        destructive: false,
        requires_admin: false,
        cost: Some("cheap".to_owned()),
        env: Vec::new(),
        limits: None,
        mcp: None,
        rest: None,
        cli: None,
        palette: None,
        ui: None,
        examples: Vec::new(),
        meta: json!({}),
    }
}

fn rest_tool(name: &str, path: &str) -> ProviderTool {
    ProviderTool {
        rest: Some(RestOverlay {
            enabled: true,
            method: Some("POST".to_owned()),
            path: Some(path.to_owned()),
            tags: Vec::new(),
            summary: None,
            description: None,
            deprecated: false,
            path_params: json!({}),
            query_params: json!({}),
            request_body_schema: None,
        }),
        ..tool(name)
    }
}

fn catalog(provider: &str, tools: Vec<ProviderTool>) -> ProviderCatalog {
    ProviderManifest {
        schema_version: 1,
        provider: ProviderIdentity {
            name: provider.to_owned(),
            kind: ProviderKind::StaticRust,
            title: None,
            description: None,
            homepage: None,
            source: None,
            version: None,
            enabled: Some(true),
        },
        tools,
        prompts: Vec::new(),
        resources: Vec::new(),
        tasks: Vec::new(),
        elicitation: Vec::new(),
        env: Vec::new(),
        capabilities: Default::default(),
        docs: None,
        plugin: None,
        ui: None,
        meta: json!({}),
    }
}

fn catalog_with_capabilities(
    provider: &str,
    tools: Vec<ProviderTool>,
    capabilities: HostCapabilities,
) -> ProviderCatalog {
    ProviderManifest {
        capabilities,
        ..catalog(provider, tools)
    }
}

#[derive(Clone)]
struct EchoProvider {
    catalog: ProviderCatalog,
    delay: Duration,
    started: Option<Arc<Notify>>,
}

#[async_trait]
impl Provider for EchoProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        if let Some(started) = &self.started {
            started.notify_one();
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(ProviderOutput::json(json!({
            "provider": call.provider,
            "action": call.action,
            "snapshot_id": call.snapshot_id,
            "message": call.params.get("message").cloned().unwrap_or(json!(null))
        })))
    }
}

fn call(action: &str, params: serde_json::Value) -> ProviderCall {
    ProviderCall {
        provider: String::new(),
        action: action.to_owned(),
        params,
        principal: ProviderPrincipal {
            subject: "alice".to_owned(),
            scopes: vec!["example:read".to_owned()],
        },
        auth_mode: ProviderAuthMode::Mounted,
        surface: ProviderSurface::Mcp,
        destructive_confirmed: false,
        limits: ProviderRequestLimits::default(),
        snapshot_id: String::new(),
    }
}

#[test]
fn snapshot_indexes_are_deterministic_and_fingerprinted() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog("demo", vec![tool("beta"), tool("alpha")]),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");
    let snapshot = registry.snapshot();

    assert_eq!(snapshot.action_names(), vec!["alpha", "beta"]);
    assert!(snapshot.fingerprint.starts_with("sha256:"));
    assert_eq!(snapshot.compiled_validator_count, 2);
}

#[test]
fn duplicate_actions_fail_snapshot_validation() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog("demo", vec![tool("dupe"), tool("dupe")]),
        delay: Duration::ZERO,
        started: None,
    });
    let error = match ProviderRegistry::new(vec![provider]) {
        Ok(_) => panic!("duplicate action should fail"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "duplicate_tool_name");
}

#[test]
fn snapshot_cached_openapi_includes_provider_rest_routes() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog("demo", vec![rest_tool("weather", "/v1/weather")]),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");
    let snapshot = registry.snapshot();
    let openapi: serde_json::Value =
        serde_json::from_slice(&snapshot.cached_openapi_bytes).expect("OpenAPI JSON");

    assert_eq!(
        openapi["paths"]["/v1/weather"]["post"]["operationId"],
        "weather"
    );
}

#[tokio::test]
async fn dispatch_uses_one_snapshot_across_reload() {
    let started = Arc::new(Notify::new());
    let old_provider = Arc::new(EchoProvider {
        catalog: catalog("old", vec![tool("echo")]),
        delay: Duration::from_millis(25),
        started: Some(started.clone()),
    });
    let registry = ProviderRegistry::new(vec![old_provider]).expect("registry");
    let old_snapshot = registry.snapshot().id.clone();

    let in_flight = {
        let registry = registry.clone();
        tokio::spawn(async move {
            registry
                .dispatch(call("echo", json!({"message": "old"})))
                .await
                .expect("dispatch")
        })
    };
    started.notified().await;

    let new_provider = Arc::new(EchoProvider {
        catalog: catalog("new", vec![tool("echo")]),
        delay: Duration::ZERO,
        started: None,
    });
    registry.reload(vec![new_provider]).expect("reload");
    let output = in_flight.await.expect("join");

    assert_eq!(output.value["snapshot_id"], old_snapshot);
    assert_ne!(registry.snapshot().id, old_snapshot);
}

#[tokio::test]
async fn input_schema_and_response_limits_are_enforced_before_and_after_provider_code() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog("demo", vec![tool("echo")]),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    let error = registry
        .dispatch(call("echo", json!({"extra": true})))
        .await
        .expect_err("schema rejects unknown input");
    assert_eq!(&*error.code, "input_schema_failed");

    let mut limited = call("echo", json!({"message": "too big"}));
    limited.limits.max_response_bytes = 8;
    let error = registry
        .dispatch(limited)
        .await
        .expect_err("response is capped");
    assert_eq!(&*error.code, "response_too_large");
}

#[tokio::test]
async fn capability_grants_must_match_requested_scope() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog_with_capabilities(
            "demo",
            vec![tool("fetch")],
            HostCapabilities {
                network: Some(NetworkCapability {
                    enabled: true,
                    allowed_hosts: vec!["api.internal.example".to_owned()],
                }),
                ..Default::default()
            },
        ),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::with_capabilities(
        vec![provider],
        CapabilityBroker::new(vec![CapabilityGrant::Network {
            allowed_hosts: vec!["other.internal.example".to_owned()],
        }]),
    )
    .expect("registry");

    let error = registry
        .dispatch(call("fetch", json!({})))
        .await
        .expect_err("mismatched network host should be denied");
    assert_eq!(&*error.code, "capability_denied");
}

#[tokio::test]
async fn matching_capability_grants_allow_requested_scope() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog_with_capabilities(
            "demo",
            vec![tool("fetch")],
            HostCapabilities {
                network: Some(NetworkCapability {
                    enabled: true,
                    allowed_hosts: vec!["api.internal.example".to_owned()],
                }),
                ..Default::default()
            },
        ),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::with_capabilities(
        vec![provider],
        CapabilityBroker::new(vec![CapabilityGrant::Network {
            allowed_hosts: vec!["api.internal.example".to_owned()],
        }]),
    )
    .expect("registry");

    let output = registry
        .dispatch(call("fetch", json!({})))
        .await
        .expect("matching network host grant should allow dispatch");
    assert_eq!(output.value["action"], "fetch");
}

#[tokio::test]
async fn admin_required_tools_reject_non_admin_principals() {
    let mut admin_tool = tool("admin_report");
    admin_tool.requires_admin = true;
    let provider = Arc::new(EchoProvider {
        catalog: catalog("demo", vec![admin_tool]),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    let error = registry
        .dispatch(call("admin_report", json!({})))
        .await
        .expect_err("non-admin principal should be denied");
    assert_eq!(&*error.code, "admin_required");

    let mut admin_call = call("admin_report", json!({}));
    admin_call.principal.scopes.push("example:admin".to_owned());
    registry
        .dispatch(admin_call)
        .await
        .expect("admin principal should be allowed");
}

#[tokio::test]
async fn provider_action_scopes_are_enforced_by_registry() {
    let mut write_tool = tool("write_note");
    write_tool.scope = Some("example:write".to_owned());
    let provider = Arc::new(EchoProvider {
        catalog: catalog("demo", vec![write_tool]),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    let error = registry
        .dispatch(call("write_note", json!({})))
        .await
        .expect_err("read-only principal should be denied");
    assert_eq!(&*error.code, "insufficient_scope");

    let mut write_call = call("write_note", json!({}));
    write_call.principal.scopes.push("example:write".to_owned());
    registry
        .dispatch(write_call)
        .await
        .expect("write scope should allow dispatch");
}

#[tokio::test]
async fn destructive_provider_actions_require_confirmation() {
    let mut destructive_tool = tool("delete_note");
    destructive_tool.destructive = true;
    let provider = Arc::new(EchoProvider {
        catalog: catalog("demo", vec![destructive_tool]),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    assert!(registry
        .snapshot()
        .action_requires_confirmation("delete_note"));
    let error = registry
        .dispatch(call("delete_note", json!({})))
        .await
        .expect_err("unconfirmed destructive action should be denied");
    assert_eq!(&*error.code, "confirmation_required");

    let mut confirmed = call("delete_note", json!({}));
    confirmed.destructive_confirmed = true;
    registry
        .dispatch(confirmed)
        .await
        .expect("confirmed destructive action should dispatch");
}
