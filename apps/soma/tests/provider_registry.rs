use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::json;
use soma_contracts::providers::{
    CapabilityGrant, HostCapabilities, McpOverlay, NetworkCapability, ProviderCatalog,
    ProviderIdentity, ProviderKind, ProviderManifest, ProviderPrompt, ProviderResource,
    ProviderTool, RestOverlay,
};
use soma_service::capabilities::CapabilityBroker;
use soma_service::provider_registry::{
    DynamicResourceTemplate, Provider, ProviderAuthMode, ProviderCall, ProviderOutput,
    ProviderPrincipal, ProviderRegistry, ProviderRequestLimits, ProviderSurface,
    ResourceReadOutput,
};
use soma_service::ProviderError;
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
        scope: Some("soma:read".to_owned()),
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

fn tool_with_output_schema(name: &str, output_schema: serde_json::Value) -> ProviderTool {
    ProviderTool {
        output_schema: Some(output_schema),
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

fn catalog_with_primitives(provider: &str) -> ProviderCatalog {
    ProviderManifest {
        prompts: vec![ProviderPrompt {
            name: "brief_prompt".to_owned(),
            description: "Prompt for a compact brief".to_owned(),
            template: Some("Summarize {{topic}} in three bullet points.".to_owned()),
            arguments_schema: Some(json!({
                "type": "object",
                "properties": {
                    "topic": { "type": "string" }
                }
            })),
            scope: Some("soma:read".to_owned()),
            mcp: None,
            examples: Vec::new(),
        }],
        resources: vec![ProviderResource {
            uri_template: "soma://demo/{id}".to_owned(),
            name: "demo_resource".to_owned(),
            description: "Demo resource".to_owned(),
            mime_type: Some("application/json".to_owned()),
            scope: Some("soma:read".to_owned()),
            mcp: None,
            annotations: json!({}),
        }],
        ..catalog(provider, vec![rest_tool("weather", "/v1/weather")])
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
            scopes: vec!["soma:read".to_owned()],
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

    let validation = snapshot.validation_summary();
    assert_eq!(validation["ok"], true);
    assert_eq!(validation["provider_count"], 1);
    assert_eq!(validation["action_count"], 2);
    assert_eq!(validation["actions"], json!(["alpha", "beta"]));

    let inspection = snapshot.inspection_report();
    assert_eq!(inspection["providers"][0]["name"], "demo");
    assert_eq!(
        inspection["providers"][0]["runtime_security"]["runtime"],
        "in-process"
    );
}

#[test]
fn inspection_report_includes_provider_routes_schemas_prompts_and_resources() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog_with_primitives("demo"),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");
    let inspection = registry.snapshot().inspection_report();
    let provider = &inspection["providers"][0];

    assert_eq!(provider["tools"][0]["name"], "weather");
    assert_eq!(provider["tools"][0]["input_schema"]["type"], "object");
    assert_eq!(provider["tools"][0]["rest"]["path"], "/v1/weather");
    assert_eq!(provider["tools"][0]["surfaces"]["rest"], true);
    assert_eq!(provider["prompts"][0]["name"], "brief_prompt");
    assert_eq!(
        provider["prompts"][0]["template"], "Summarize {{topic}} in three bullet points.",
        "inspection_report must include prompt.template — it's the field \
         a remote-adapter server's RemoteCatalogProvider reconstructs \
         ProviderPrompt.template from"
    );
    assert_eq!(
        provider["prompts"][0]["arguments_schema"]["properties"]["topic"]["type"],
        "string"
    );
    assert_eq!(provider["resources"][0]["name"], "demo_resource");
    assert_eq!(provider["resources"][0]["uri_template"], "soma://demo/{id}");
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
fn invalid_output_schema_fails_snapshot_validation() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog(
            "demo",
            vec![tool_with_output_schema("broken", json!({ "type": 42 }))],
        ),
        delay: Duration::ZERO,
        started: None,
    });
    let error = match ProviderRegistry::new(vec![provider]) {
        Ok(_) => panic!("invalid output schema should fail"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "output_schema_invalid");
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
async fn output_schema_is_enforced_after_provider_code() {
    let provider = Arc::new(EchoProvider {
        catalog: catalog(
            "demo",
            vec![
                tool_with_output_schema(
                    "checked_echo",
                    json!({
                        "type": "object",
                        "additionalProperties": true,
                        "required": ["message"],
                        "properties": {
                            "message": { "type": "string" }
                        }
                    }),
                ),
                tool_with_output_schema(
                    "bad_echo",
                    json!({
                        "type": "object",
                        "additionalProperties": true,
                        "required": ["ok"],
                        "properties": {
                            "ok": { "type": "boolean" }
                        }
                    }),
                ),
            ],
        ),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");
    assert_eq!(registry.snapshot().compiled_validator_count, 4);

    let output = registry
        .dispatch(call("checked_echo", json!({"message": "valid"})))
        .await
        .expect("matching output schema should allow dispatch");
    assert_eq!(output.value["message"], "valid");

    let error = registry
        .dispatch(call("bad_echo", json!({"message": "invalid"})))
        .await
        .expect_err("provider output should be schema checked");
    assert_eq!(&*error.code, "output_schema_failed");
    let error_json = serde_json::to_value(&error).expect("provider error serializes");
    assert_eq!(error_json["phase"], "output_validation");
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
    admin_call.principal.scopes.push("soma:admin".to_owned());
    registry
        .dispatch(admin_call)
        .await
        .expect("admin principal should be allowed");
}

#[tokio::test]
async fn provider_action_scopes_are_enforced_by_registry() {
    let mut write_tool = tool("write_note");
    write_tool.scope = Some("soma:write".to_owned());
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
    write_call.principal.scopes.push("soma:write".to_owned());
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

/// A `Provider` whose only job is to serve resources — either a fixed static
/// catalog resource, or a dynamic template that echoes captured params back
/// as text, letting these tests exercise `ProviderRegistry::read_resource`
/// without going through the filesystem.
#[derive(Clone)]
struct ResourceProvider {
    catalog: ProviderCatalog,
    dynamic_template: Option<DynamicResourceTemplate>,
}

#[async_trait]
impl Provider for ResourceProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Err(ProviderError::validation(
            &self.catalog.provider.name,
            &call.action,
            "no_actions",
            "this test provider has no callable actions",
        ))
    }

    fn dynamic_resource_templates(&self) -> Vec<DynamicResourceTemplate> {
        self.dynamic_template.clone().into_iter().collect()
    }

    fn supports_resource_reads(&self) -> bool {
        true
    }

    async fn read_resource(
        &self,
        uri: &str,
        params: &std::collections::BTreeMap<String, String>,
    ) -> Result<ResourceReadOutput, ProviderError> {
        if let Some(resource) = self
            .catalog
            .resources
            .iter()
            .find(|r| r.uri_template == uri)
        {
            return Ok(ResourceReadOutput::Text {
                text: format!("static:{}", resource.name),
                mime_type: resource.mime_type.clone(),
            });
        }
        if params.is_empty() {
            return Ok(ResourceReadOutput::Text {
                text: format!("dynamic:{uri}"),
                mime_type: None,
            });
        }
        let rendered = params
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        Ok(ResourceReadOutput::Text {
            text: format!("dynamic:{rendered}"),
            mime_type: None,
        })
    }
}

fn resource_catalog(provider: &str, resource: ProviderResource) -> ProviderCatalog {
    ProviderManifest {
        resources: vec![resource],
        ..catalog(provider, Vec::new())
    }
}

fn demo_resource(scope: Option<&str>) -> ProviderResource {
    ProviderResource {
        uri_template: "soma://resources/runbook".to_owned(),
        name: "runbook".to_owned(),
        description: "Runbook".to_owned(),
        mime_type: Some("text/markdown".to_owned()),
        scope: scope.map(str::to_owned),
        mcp: None,
        annotations: json!({}),
    }
}

#[tokio::test]
async fn static_resource_read_returns_provider_content() {
    let provider = Arc::new(ResourceProvider {
        catalog: resource_catalog("demo", demo_resource(None)),
        dynamic_template: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    let output = registry
        .read_resource(
            "soma://resources/runbook",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect("static resource should resolve");
    match output {
        ResourceReadOutput::Text { text, .. } => assert_eq!(text, "static:runbook"),
        ResourceReadOutput::Blob { .. } => panic!("expected text output"),
    }
}

#[tokio::test]
async fn unknown_resource_uri_is_rejected() {
    let provider = Arc::new(ResourceProvider {
        catalog: catalog("demo", Vec::new()),
        dynamic_template: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    let error = registry
        .read_resource(
            "soma://resources/missing",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect_err("unknown resource must be rejected");
    assert_eq!(&*error.code, "unknown_resource");
}

#[tokio::test]
async fn resource_scope_is_enforced_by_registry() {
    let provider = Arc::new(ResourceProvider {
        catalog: resource_catalog("demo", demo_resource(Some("soma:write"))),
        dynamic_template: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    let error = registry
        .read_resource(
            "soma://resources/runbook",
            &ProviderPrincipal {
                subject: "reader".to_owned(),
                scopes: vec!["soma:read".to_owned()],
            },
            ProviderAuthMode::Mounted,
        )
        .await
        .expect_err("read-only principal should be denied");
    assert_eq!(&*error.code, "insufficient_scope");

    registry
        .read_resource(
            "soma://resources/runbook",
            &ProviderPrincipal {
                subject: "writer".to_owned(),
                scopes: vec!["soma:write".to_owned()],
            },
            ProviderAuthMode::Mounted,
        )
        .await
        .expect("write scope should satisfy a write-scoped resource");
}

#[tokio::test]
async fn resource_scope_is_ignored_outside_mounted_auth() {
    let provider = Arc::new(ResourceProvider {
        catalog: resource_catalog("demo", demo_resource(Some("soma:write"))),
        dynamic_template: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    registry
        .read_resource(
            "soma://resources/runbook",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect("scope should not be enforced outside Mounted auth, matching tool enforce_scope");
}

#[tokio::test]
async fn duplicate_resource_uri_fails_snapshot_validation() {
    // Different resource *names* so the pre-existing name-based
    // `duplicate_mcp_primitive` check doesn't fire first — this isolates the
    // URI-based check this test actually targets.
    let mut second_resource = demo_resource(None);
    second_resource.name = "runbook-2".to_owned();
    let first = Arc::new(ResourceProvider {
        catalog: resource_catalog("first", demo_resource(None)),
        dynamic_template: None,
    });
    let second = Arc::new(ResourceProvider {
        catalog: resource_catalog("second", second_resource),
        dynamic_template: None,
    });
    let error = match ProviderRegistry::new(vec![first, second]) {
        Ok(_) => panic!("duplicate resource URI should fail"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "duplicate_resource_uri");
}

#[tokio::test]
async fn dynamic_resource_template_matches_and_captures_params() {
    let provider = Arc::new(ResourceProvider {
        catalog: catalog("demo", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(
                &["service", "[name]"],
                "service-status",
                "Live service status",
                None,
            )
            .expect("valid template"),
        ),
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");

    assert_eq!(
        registry.snapshot().dynamic_resource_templates()[0]
            .1
            .uri_template(),
        "soma://resources/service/{name}"
    );

    let output = registry
        .read_resource(
            "soma://resources/service/checkout",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect("dynamic template should match");
    match output {
        ResourceReadOutput::Text { text, .. } => assert_eq!(text, "dynamic:name=checkout"),
        ResourceReadOutput::Blob { .. } => panic!("expected text output"),
    }
}

#[tokio::test]
async fn ambiguous_dynamic_resource_templates_fail_snapshot_validation() {
    let first = Arc::new(ResourceProvider {
        catalog: catalog("first", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(
                &["service", "[name]"],
                "by-name",
                "d",
                None,
            )
            .expect("valid template"),
        ),
    });
    let second = Arc::new(ResourceProvider {
        catalog: catalog("second", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(&["service", "[id]"], "by-id", "d", None)
                .expect("valid template"),
        ),
    });
    let error = match ProviderRegistry::new(vec![first, second]) {
        Ok(_) => panic!("ambiguous dynamic resource templates should fail"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "ambiguous_resource_template");
}

#[tokio::test]
async fn provider_declaring_resources_without_read_support_is_excluded_from_live_serving() {
    // EchoProvider inherits Provider::supports_resource_reads()'s `false`
    // default (unlike ResourceProvider above, which overrides it) — the
    // same situation as an OpenAPI/MCP/ai-sdk/WASM/Python provider
    // declaring a manifest `resources[]` field it has no way to serve.
    // Registration must still succeed (inspection/reporting legitimately
    // reads `catalog().resources` directly, regardless of live-serving
    // support) but the resource must not be listed or readable — the
    // previously-broken outcome was listing it and then always failing the
    // read.
    let provider = Arc::new(EchoProvider {
        catalog: resource_catalog("echo", demo_resource(None)),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registration should succeed");

    assert!(
        registry
            .snapshot()
            .match_resource("soma://resources/runbook")
            .is_none(),
        "a resource from a provider that can't serve reads must not be live-matchable"
    );
    let error = registry
        .read_resource(
            "soma://resources/runbook",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect_err("unreachable resource must be rejected as unknown, not attempted");
    assert_eq!(&*error.code, "unknown_resource");
}

#[tokio::test]
async fn resource_precedence_prefers_exact_dynamic_template_over_parameterized() {
    let exact = Arc::new(ResourceProvider {
        catalog: catalog("exact", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(
                &["service", "status"],
                "status",
                "d",
                None,
            )
            .expect("valid template"),
        ),
    });
    let parameterized = Arc::new(ResourceProvider {
        catalog: catalog("parameterized", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(
                &["service", "[name]"],
                "by-name",
                "d",
                None,
            )
            .expect("valid template"),
        ),
    });
    let registry = ProviderRegistry::new(vec![exact, parameterized]).expect("registry");

    let output = registry
        .read_resource(
            "soma://resources/service/status",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect("should match the exact template");
    match output {
        ResourceReadOutput::Text { text, .. } => assert_eq!(
            text, "dynamic:soma://resources/service/status",
            "the exact (zero-param) template must win over a parameterized one for the same concrete URI"
        ),
        ResourceReadOutput::Blob { .. } => panic!("expected text output"),
    }

    let output = registry
        .read_resource(
            "soma://resources/service/other",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect("should match the parameterized template");
    match output {
        ResourceReadOutput::Text { text, .. } => assert_eq!(text, "dynamic:name=other"),
        ResourceReadOutput::Blob { .. } => panic!("expected text output"),
    }
}

#[tokio::test]
async fn exact_resource_ambiguous_with_zero_param_dynamic_template_fails_snapshot_validation() {
    // A static, exact `catalog().resources[]` entry and a zero-param
    // dynamic template rendering to the same URI are just as ambiguous as
    // two same-shape dynamic templates — the exact tier would otherwise
    // silently and permanently shadow the dynamic one at read time.
    let static_provider = Arc::new(ResourceProvider {
        catalog: resource_catalog(
            "static",
            ProviderResource {
                uri_template: "soma://resources/service/status".to_owned(),
                name: "status".to_owned(),
                description: "d".to_owned(),
                mime_type: None,
                scope: None,
                mcp: None,
                annotations: json!({}),
            },
        ),
        dynamic_template: None,
    });
    let dynamic_provider = Arc::new(ResourceProvider {
        catalog: catalog("dynamic", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(
                &["service", "status"],
                "status",
                "d",
                None,
            )
            .expect("valid template"),
        ),
    });
    let error = match ProviderRegistry::new(vec![static_provider, dynamic_provider]) {
        Ok(_) => panic!(
            "cross-tier ambiguity between an exact resource and a dynamic template should fail"
        ),
        Err(error) => error,
    };
    assert_eq!(error.code(), "ambiguous_resource_template");
}

#[tokio::test]
async fn unreadable_provider_resource_is_excluded_from_exact_resources_snapshot_view() {
    // EchoProvider inherits the `false` supports_resource_reads() default —
    // regression for a Codex review finding on rmcp-template-7nyf: `list_resources`
    // must be built from the same live index `read_resource` consults
    // (`RegistrySnapshot::exact_resources()`), not raw `catalogs`, or it
    // can advertise a resource that always fails to read.
    let provider = Arc::new(EchoProvider {
        catalog: resource_catalog("echo", demo_resource(None)),
        delay: Duration::ZERO,
        started: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registration should succeed");
    assert!(
        registry
            .snapshot()
            .exact_resources()
            .all(|resource| resource.uri_template != "soma://resources/runbook"),
        "a resource from a provider that can't serve reads must not appear in the live \
         exact_resources() view that resources/list is built from"
    );
}

#[tokio::test]
async fn mcp_disabled_resource_is_excluded_from_live_resources_surface() {
    // Regression for a Codex review finding: `mcp: { enabled: false }` must
    // be honored the same way tools/prompts honor their MCP overlay --
    // never indexed for live resources/list or resources/read, even though
    // the owning provider otherwise supports reads.
    let mut resource = demo_resource(None);
    resource.mcp = Some(McpOverlay {
        enabled: false,
        title: None,
        annotations: json!({}),
    });
    let provider = Arc::new(ResourceProvider {
        catalog: resource_catalog("demo", resource),
        dynamic_template: None,
    });
    let registry = ProviderRegistry::new(vec![provider]).expect("registration should succeed");

    assert!(
        registry
            .snapshot()
            .exact_resources()
            .all(|resource| resource.uri_template != "soma://resources/runbook"),
        "an mcp-disabled resource must not appear in exact_resources()"
    );
    let error = registry
        .read_resource(
            "soma://resources/runbook",
            &ProviderPrincipal::anonymous(),
            ProviderAuthMode::LoopbackDev,
        )
        .await
        .expect_err("an mcp-disabled resource must not be readable via MCP");
    assert_eq!(&*error.code, "unknown_resource");
}

#[tokio::test]
async fn overlapping_parameterized_templates_with_literals_in_different_positions_are_ambiguous() {
    // Regression for a Codex review finding: `foo/[id]` and `[kind]/bar`
    // have different "shapes" (literal-then-param vs param-then-literal)
    // but both match the concrete request `foo/bar` -- a shape-equality-only
    // check misses this overlap.
    let first = Arc::new(ResourceProvider {
        catalog: catalog("first", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(&["foo", "[id]"], "by-id", "d", None)
                .expect("valid template"),
        ),
    });
    let second = Arc::new(ResourceProvider {
        catalog: catalog("second", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(&["[kind]", "bar"], "by-kind", "d", None)
                .expect("valid template"),
        ),
    });
    let error = match ProviderRegistry::new(vec![first, second]) {
        Ok(_) => panic!(
            "templates that could both match the same concrete URI (e.g. foo/bar) must be \
             rejected as ambiguous even when their literal falls in different positions"
        ),
        Err(error) => error,
    };
    assert_eq!(error.code(), "ambiguous_resource_template");
}

#[tokio::test]
async fn non_overlapping_parameterized_templates_coexist() {
    // Sanity check alongside the ambiguity regression above: templates that
    // genuinely can never match the same concrete URI (a literal/literal
    // mismatch at some position) must still be allowed to coexist.
    let first = Arc::new(ResourceProvider {
        catalog: catalog("first", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(&["service", "[name]"], "a", "d", None)
                .expect("valid template"),
        ),
    });
    let second = Arc::new(ResourceProvider {
        catalog: catalog("second", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(&["other", "[id]"], "b", "d", None)
                .expect("valid template"),
        ),
    });
    ProviderRegistry::new(vec![first, second])
        .expect("non-overlapping parameterized templates must coexist");
}

#[tokio::test]
async fn parameterized_and_catch_all_templates_coexist_across_precedence_tiers() {
    // A parameterized template and a catch-all template can match the same
    // concrete URI (e.g. `service/x` matches both `service/[name]` and
    // `service/[...rest]`), but that's resolved deterministically by
    // match_resource's precedence order (parameterized wins), not an error
    // -- must not regress into a false-positive ambiguity rejection.
    let parameterized = Arc::new(ResourceProvider {
        catalog: catalog("parameterized", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(&["service", "[name]"], "a", "d", None)
                .expect("valid template"),
        ),
    });
    let catch_all = Arc::new(ResourceProvider {
        catalog: catalog("catch-all", Vec::new()),
        dynamic_template: Some(
            DynamicResourceTemplate::from_path_segments(&["service", "[...rest]"], "b", "d", None)
                .expect("valid template"),
        ),
    });
    ProviderRegistry::new(vec![parameterized, catch_all])
        .expect("parameterized and catch-all templates at different precedence tiers must coexist");
}
