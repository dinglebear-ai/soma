use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};
use soma_client::SomaClient;
use soma_config::SomaConfig;
use soma_domain::{
    AuthorizationMode, Confirmation, Principal, RequestId, ScopeSet, Surface, TraceContext,
    scopes::{READ_SCOPE, WRITE_SCOPE},
};
use soma_provider_core::{ProviderCatalog, ProviderResource};

use super::{
    CodeModeExecuteRequest, ExecuteActionRequest, GatewayExecuteRequest, GatewayReloadRequest,
    OpenApiExecuteRequest, ScaffoldIntentRequest, SomaApplication,
};
use crate::{
    ApplicationError, ApplicationErrorDetails, ApplicationPorts, CodeModePort,
    DynamicResourceTemplate, ExecutionContext, GatewayPort, OpenApiPort, PortError, ProviderCall,
    ProviderError, ProviderOutput, ProviderRegistry, PythonEnvironmentPort, SomaService,
    StaticRustProvider, provider_registry::Provider,
};

struct RecordingProvider {
    catalog: ProviderCatalog,
    output: Value,
    calls: Mutex<Vec<ProviderCall>>,
}

struct BlockingGenerationProvider {
    catalog: ProviderCatalog,
    label: &'static str,
    entered: Option<Arc<tokio::sync::Notify>>,
    release: Option<Arc<tokio::sync::Notify>>,
}

#[async_trait]
impl Provider for BlockingGenerationProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        if let Some(entered) = &self.entered {
            entered.notify_one();
        }
        if let Some(release) = &self.release {
            release.notified().await;
        }
        Ok(ProviderOutput::json(json!({"echo": self.label})))
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, crate::ProviderError> {
        self.calls.lock().unwrap().push(call);
        Ok(ProviderOutput::json(self.output.clone()))
    }

    fn runtime_status(&self) -> Option<Value> {
        Some(json!({"running": true, "logs": []}))
    }

    fn cancel_active(&self) -> bool {
        true
    }

    fn dynamic_resource_templates(&self) -> Vec<DynamicResourceTemplate> {
        let mut template = DynamicResourceTemplate::from_path_segments(
            &["recording", "[id]"],
            "recording item",
            "A scoped dynamic resource",
            Some("text/markdown".to_owned()),
        )
        .unwrap();
        template.scope = Some(WRITE_SCOPE.to_owned());
        vec![template]
    }

    fn supports_resource_reads(&self) -> bool {
        true
    }
}

#[derive(Default)]
struct RecordingEngines {
    calls: Mutex<Vec<(String, String, Option<String>)>>,
}

impl RecordingEngines {
    fn record(&self, operation: &str, context: &ExecutionContext) -> Value {
        let traceparent = context
            .trace
            .as_ref()
            .and_then(|trace| trace.traceparent.clone());
        self.calls.lock().unwrap().push((
            operation.to_owned(),
            context.request_id.as_str().to_owned(),
            traceparent,
        ));
        json!({"operation": operation})
    }
}

#[async_trait]
impl GatewayPort for RecordingEngines {
    async fn status(&self, context: &ExecutionContext) -> Result<Value, PortError> {
        Ok(self.record("gateway.status", context))
    }

    async fn reload(
        &self,
        _request: GatewayReloadRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(self.record("gateway.reload", context))
    }

    async fn execute(
        &self,
        request: GatewayExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(self.record(&format!("gateway.{}", request.action), context))
    }

    async fn list_mcp_tools(
        &self,
        _scope: Option<&crate::GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<crate::GatewayToolRoute>, PortError> {
        Ok(Vec::new())
    }

    async fn call_mcp_tool(
        &self,
        _name: &str,
        _params: Value,
        _scope: Option<&crate::GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok(None)
    }

    async fn list_mcp_resources(
        &self,
        _scope: Option<&crate::GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<crate::GatewayResourceRoute>, PortError> {
        Ok(Vec::new())
    }

    async fn read_mcp_resource(
        &self,
        _uri: &str,
        _scope: Option<&crate::GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok(None)
    }

    async fn list_mcp_prompts(
        &self,
        _scope: Option<&crate::GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<crate::GatewayPromptRoute>, PortError> {
        Ok(Vec::new())
    }

    async fn get_mcp_prompt(
        &self,
        _name: &str,
        _arguments: Option<serde_json::Map<String, Value>>,
        _scope: Option<&crate::GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok(None)
    }
}

#[async_trait]
impl CodeModePort for RecordingEngines {
    async fn execute(
        &self,
        _request: CodeModeExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(self.record("codemode.execute", context))
    }
}

#[async_trait]
impl OpenApiPort for RecordingEngines {
    async fn execute(
        &self,
        request: OpenApiExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(self.record(&format!("openapi.{}", request.operation), context))
    }
}

#[derive(Default)]
struct RecordingPythonEnvironments {
    calls: Mutex<Vec<String>>,
}

#[async_trait]
impl PythonEnvironmentPort for RecordingPythonEnvironments {
    async fn status(&self) -> Result<Value, PortError> {
        self.calls.lock().unwrap().push("status".to_owned());
        Ok(json!({"entries": [{"state": "ready"}]}))
    }

    async fn prune(
        &self,
        stale_before_unix_seconds: u64,
        max_entries: usize,
        apply: bool,
    ) -> Result<Value, PortError> {
        self.calls.lock().unwrap().push(format!(
            "prune:{stale_before_unix_seconds}:{max_entries}:{apply}"
        ));
        Ok(json!({"apply": apply, "selected": max_entries}))
    }

    async fn repair(&self, _provider_path: &std::path::Path) -> Result<Value, PortError> {
        unreachable!("repair is covered by the lifecycle tests")
    }

    async fn update(
        &self,
        _provider_path: &std::path::Path,
    ) -> Result<crate::PythonEnvironmentUpdateCandidate, PortError> {
        unreachable!("update activation is covered by registry lifecycle tests")
    }
}

fn application(
    destructive: bool,
    output: Value,
) -> (
    SomaApplication,
    Arc<RecordingProvider>,
    Arc<RecordingEngines>,
) {
    let mut catalog = StaticRustProvider::catalog_static();
    catalog.provider.name = "recording".to_owned();
    catalog
        .tools
        .retain(|tool| tool.name == "echo" || tool.name.starts_with("python_"));
    catalog.tools[0].destructive = destructive;
    catalog.prompts[0].template = Some("Run {{action}}".to_owned());
    catalog.prompts[0].scope = Some(READ_SCOPE.to_owned());
    catalog.resources.push(ProviderResource {
        uri_template: "soma://resources/recording/runbook.md".to_owned(),
        name: "recording runbook".to_owned(),
        description: "A scoped exact resource".to_owned(),
        mime_type: Some("text/markdown".to_owned()),
        scope: Some(WRITE_SCOPE.to_owned()),
        mcp: None,
        annotations: json!({}),
    });
    let provider = Arc::new(RecordingProvider {
        catalog,
        output,
        calls: Mutex::new(Vec::new()),
    });
    let registry = ProviderRegistry::new(vec![provider.clone()]).unwrap();
    let service = SomaService::new(SomaClient::new(&SomaConfig::default()).unwrap());
    let engines = Arc::new(RecordingEngines::default());
    let ports = ApplicationPorts::unavailable()
        .with_gateway(engines.clone())
        .with_codemode(engines.clone())
        .with_openapi(engines.clone());
    (
        SomaApplication::new(Arc::new(service), Arc::new(registry), ports),
        provider,
        engines,
    )
}

fn application_with_python_environments(
    environments: Arc<RecordingPythonEnvironments>,
) -> SomaApplication {
    let service = SomaService::new(SomaClient::new(&SomaConfig::default()).unwrap());
    let registry =
        ProviderRegistry::new(vec![Arc::new(StaticRustProvider::new(service.clone()))]).unwrap();
    let ports = ApplicationPorts::unavailable().with_python_environment(environments);
    SomaApplication::new(Arc::new(service), Arc::new(registry), ports)
}

fn mounted_context(confirmation: Confirmation, response_limit: Option<usize>) -> ExecutionContext {
    ExecutionContext {
        principal: Some(Principal::new("user-1", ScopeSet::from([READ_SCOPE]))),
        authorization_mode: AuthorizationMode::Mounted,
        surface: Surface::Rest,
        trace: None,
        destructive_confirmation: confirmation,
        response_limit,
        request_id: RequestId::new("request-1").unwrap(),
    }
}

fn execute_echo() -> ExecuteActionRequest {
    ExecuteActionRequest {
        action: "echo".to_owned(),
        params: json!({"message": "hello"}),
    }
}

#[tokio::test]
async fn execute_action_enforces_mounted_authorization() {
    let (application, _, _) = application(false, json!({"echo": "hello"}));
    let mut context = mounted_context(Confirmation::Missing, None);
    context.principal = Some(Principal::anonymous());

    let error = application
        .execute_action(execute_echo(), context)
        .await
        .unwrap_err();

    assert_eq!(error.code, "insufficient_scope");
    assert_eq!(
        error.remediation,
        "Authenticate with a token that includes the required scope."
    );
}

#[tokio::test]
async fn execute_action_enforces_destructive_confirmation() {
    let (application, _, _) = application(true, json!({"echo": "hello"}));

    let error = application
        .execute_action(execute_echo(), mounted_context(Confirmation::Missing, None))
        .await
        .unwrap_err();

    assert_eq!(error.code, "confirmation_required");
}

#[tokio::test]
async fn python_environment_status_uses_the_shared_operator_port() {
    let environments = Arc::new(RecordingPythonEnvironments::default());
    let application = application_with_python_environments(environments.clone());
    let mut context = mounted_context(Confirmation::Missing, None);
    context.principal = Some(Principal::new(
        "operator",
        ScopeSet::from([READ_SCOPE, WRITE_SCOPE, "soma:admin"]),
    ));

    let response = application
        .execute_action(
            ExecuteActionRequest {
                action: "python_environment_status".to_owned(),
                params: json!({}),
            },
            context,
        )
        .await
        .unwrap();

    assert_eq!(response.output["entries"][0]["state"], "ready");
    assert_eq!(environments.calls.lock().unwrap().as_slice(), ["status"]);
}

#[tokio::test]
async fn trusted_loopback_cli_can_run_admin_operator_status() {
    let environments = Arc::new(RecordingPythonEnvironments::default());
    let application = application_with_python_environments(environments.clone());
    let context =
        ExecutionContext::loopback(Surface::Cli, RequestId::new("local-python-status").unwrap());

    let response = application
        .execute_action(
            ExecuteActionRequest {
                action: "python_environment_status".to_owned(),
                params: json!({}),
            },
            context,
        )
        .await
        .expect("trusted local status bypasses mounted admin enforcement");

    assert_eq!(response.output["entries"][0]["state"], "ready");
    assert_eq!(environments.calls.lock().unwrap().as_slice(), ["status"]);
}

#[tokio::test]
async fn python_environment_prune_requires_write_scope_and_confirmation() {
    let environments = Arc::new(RecordingPythonEnvironments::default());
    let application = application_with_python_environments(environments.clone());
    let request = || ExecuteActionRequest {
        action: "python_environment_prune".to_owned(),
        params: json!({
            "stale_before_unix_seconds": 42,
            "max_entries": 3
        }),
    };

    let error = application
        .execute_action(request(), mounted_context(Confirmation::Confirmed, None))
        .await
        .unwrap_err();
    assert_eq!(error.code, "insufficient_scope");

    let mut context = mounted_context(Confirmation::Missing, None);
    context.principal = Some(Principal::new(
        "operator",
        ScopeSet::from([READ_SCOPE, WRITE_SCOPE]),
    ));
    let error = application
        .execute_action(request(), context.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code, "confirmation_required");

    context.destructive_confirmation = Confirmation::Confirmed;
    let response = application
        .execute_action(request(), context)
        .await
        .unwrap();
    assert_eq!(response.output, json!({"apply": true, "selected": 3}));
    assert_eq!(
        environments.calls.lock().unwrap().as_slice(),
        ["prune:42:3:true"]
    );
}

#[tokio::test]
async fn python_worker_status_and_cancel_share_registry_authorization() {
    let (application, _, _) = application(false, json!({}));
    let error = application
        .execute_action(
            ExecuteActionRequest {
                action: "python_worker_status".to_owned(),
                params: json!({}),
            },
            mounted_context(Confirmation::Missing, None),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "insufficient_scope");

    let mut status_context = mounted_context(Confirmation::Missing, None);
    status_context.principal = Some(Principal::new(
        "operator",
        ScopeSet::from([READ_SCOPE, WRITE_SCOPE, "soma:admin"]),
    ));
    let status = application
        .execute_action(
            ExecuteActionRequest {
                action: "python_worker_status".to_owned(),
                params: json!({}),
            },
            status_context,
        )
        .await
        .unwrap();
    assert_eq!(status.output["workers"][0]["provider"], "recording");

    let request = || ExecuteActionRequest {
        action: "python_worker_cancel".to_owned(),
        params: json!({"provider": "recording"}),
    };
    let error = application
        .execute_action(request(), mounted_context(Confirmation::Confirmed, None))
        .await
        .unwrap_err();
    assert_eq!(error.code, "insufficient_scope");

    let mut context = mounted_context(Confirmation::Confirmed, None);
    context.principal = Some(Principal::new(
        "operator",
        ScopeSet::from([READ_SCOPE, WRITE_SCOPE]),
    ));
    let cancelled = application
        .execute_action(request(), context)
        .await
        .unwrap();
    assert_eq!(cancelled.output["cancelled"], true);
}

#[tokio::test]
async fn python_control_prefix_does_not_shadow_dynamic_provider_actions() {
    let mut catalog = StaticRustProvider::catalog_static();
    catalog.provider.name = "prefix-provider".to_owned();
    catalog.tools.retain(|tool| tool.name == "echo");
    catalog.tools[0].name = "python_environment_status_extra".to_owned();
    let provider = Arc::new(RecordingProvider {
        catalog,
        output: json!({"echo": "provider-owned"}),
        calls: Mutex::new(Vec::new()),
    });
    let registry = ProviderRegistry::new(vec![provider.clone()]).unwrap();
    let service = SomaService::new(SomaClient::new(&SomaConfig::default()).unwrap());
    let application = SomaApplication::new(
        Arc::new(service),
        Arc::new(registry),
        ApplicationPorts::unavailable(),
    );

    let response = application
        .execute_action(
            ExecuteActionRequest {
                action: "python_environment_status_extra".to_owned(),
                params: json!({"message": "provider-owned"}),
            },
            ExecutionContext::loopback(
                Surface::Rest,
                RequestId::new("python-prefix-provider").unwrap(),
            ),
        )
        .await
        .expect("prefixed dynamic action should dispatch normally");

    assert_eq!(response.output, json!({"echo": "provider-owned"}));
    assert_eq!(provider.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn python_generation_rollback_requires_write_scope_and_confirmation() {
    let (application, _, _) = application(false, json!({}));
    let status = application
        .execute_action(
            ExecuteActionRequest {
                action: "python_generation_status".to_owned(),
                params: json!({}),
            },
            mounted_context(Confirmation::Missing, None),
        )
        .await
        .unwrap();
    assert_eq!(status.output["active"]["generation_id"], 1);

    let request = || ExecuteActionRequest {
        action: "python_generation_rollback".to_owned(),
        params: json!({"generation_id": 1}),
    };
    let error = application
        .execute_action(request(), mounted_context(Confirmation::Confirmed, None))
        .await
        .unwrap_err();
    assert_eq!(error.code, "insufficient_scope");

    let mut context = mounted_context(Confirmation::Missing, None);
    context.principal = Some(Principal::new(
        "operator",
        ScopeSet::from([READ_SCOPE, WRITE_SCOPE]),
    ));
    let error = application
        .execute_action(request(), context)
        .await
        .unwrap_err();
    assert_eq!(error.code, "confirmation_required");
}

#[tokio::test]
async fn generation_swap_keeps_in_flight_work_on_original_provider() {
    let catalog = || {
        let mut catalog = StaticRustProvider::catalog_static();
        catalog.provider.name = "generation-provider".to_owned();
        catalog.tools.retain(|tool| tool.name == "echo");
        catalog
    };
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let old: Arc<dyn Provider> = Arc::new(BlockingGenerationProvider {
        catalog: catalog(),
        label: "old",
        entered: Some(entered.clone()),
        release: Some(release.clone()),
    });
    let registry = ProviderRegistry::new(vec![old]).unwrap();
    let active_call = {
        let registry = registry.clone();
        tokio::spawn(async move {
            registry
                .dispatch(ProviderCall {
                    provider: String::new(),
                    action: "echo".to_owned(),
                    params: json!({"message": "started"}),
                    principal: crate::ProviderPrincipal::loopback_dev(),
                    auth_mode: crate::ProviderAuthMode::LoopbackDev,
                    surface: crate::ProviderSurface::Rest,
                    destructive_confirmed: false,
                    limits: crate::ProviderRequestLimits::default(),
                    snapshot_id: String::new(),
                    request_id: String::new(),
                    traceparent: None,
                    tracestate: None,
                    progress: Default::default(),
                })
                .await
        })
    };
    entered.notified().await;

    let replacement: Arc<dyn Provider> = Arc::new(BlockingGenerationProvider {
        catalog: catalog(),
        label: "new",
        entered: None,
        release: None,
    });
    registry.reload(vec![replacement]).unwrap();
    let fresh = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "echo".to_owned(),
            params: json!({"message": "fresh"}),
            principal: crate::ProviderPrincipal::loopback_dev(),
            auth_mode: crate::ProviderAuthMode::LoopbackDev,
            surface: crate::ProviderSurface::Rest,
            destructive_confirmed: false,
            limits: crate::ProviderRequestLimits::default(),
            snapshot_id: String::new(),
            request_id: String::new(),
            traceparent: None,
            tracestate: None,
            progress: Default::default(),
        })
        .await
        .unwrap();
    assert_eq!(fresh.value["echo"], "new");

    release.notify_one();
    let original = active_call.await.unwrap().unwrap();
    assert_eq!(original.value["echo"], "old");
}

#[tokio::test]
async fn execute_action_applies_defaults_and_returns_request_context() {
    let (application, provider, _) = application(false, json!({"echo": "hello"}));

    let response = application
        .execute_action(execute_echo(), mounted_context(Confirmation::Missing, None))
        .await
        .unwrap();

    assert_eq!(response.output, json!({"echo": "hello"}));
    assert_eq!(response.request_id, "request-1");
    let calls = provider.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].surface, crate::ProviderSurface::Rest);
    assert!(!calls[0].snapshot_id.is_empty());
}

#[tokio::test]
async fn engine_operations_enforce_context_response_limit() {
    let (application, _, _) = application(false, json!({"echo": "hello"}));
    let mut context =
        ExecutionContext::loopback(Surface::Cli, RequestId::new("engine-request").unwrap());
    context.response_limit = Some(8);

    let error = application.gateway_status(context).await.unwrap_err();

    assert_eq!(error.code, "response_too_large");
}

#[tokio::test]
async fn execute_action_enforces_context_response_limit() {
    let (application, _, _) = application(false, json!({"echo": "a long response"}));

    let error = application
        .execute_action(
            execute_echo(),
            mounted_context(Confirmation::Missing, Some(8)),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code, "response_too_large");
}

#[tokio::test]
async fn execute_action_normalizes_registry_errors() {
    let (application, _, _) = application(false, json!({"echo": "hello"}));

    let error = application
        .execute_action(
            ExecuteActionRequest {
                action: "missing".to_owned(),
                params: json!({}),
            },
            mounted_context(Confirmation::Missing, None),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code, "unknown_action");
    assert!(error.message.contains("missing"));
}

#[tokio::test]
async fn engine_operations_propagate_request_and_trace_context() {
    let (application, _, engines) = application(false, json!({"echo": "hello"}));
    let mut context =
        ExecutionContext::loopback(Surface::Cli, RequestId::new("engine-request").unwrap());
    context.trace = Some(TraceContext {
        traceparent: Some("00-12345678901234567890123456789012-1234567890123456-01".to_owned()),
        tracestate: None,
    });

    application.gateway_status(context.clone()).await.unwrap();
    application
        .gateway_reload(GatewayReloadRequest { config: json!({}) }, context.clone())
        .await
        .unwrap();
    application
        .gateway_execute(
            GatewayExecuteRequest {
                action: "list".to_owned(),
                params: json!({}),
            },
            context.clone(),
        )
        .await
        .unwrap();
    application
        .codemode_execute(
            CodeModeExecuteRequest {
                source: "async () => 1".to_owned(),
                input: json!({}),
            },
            context.clone(),
        )
        .await
        .unwrap();
    application
        .openapi_execute(
            OpenApiExecuteRequest {
                operation: "getStatus".to_owned(),
                params: json!({}),
            },
            context,
        )
        .await
        .unwrap();

    let calls = engines.calls.lock().unwrap();
    assert_eq!(calls.len(), 5);
    assert!(calls.iter().all(|(_, request_id, traceparent)| {
        request_id == "engine-request"
            && traceparent
                .as_deref()
                .is_some_and(|value| value.starts_with("00-"))
    }));
}

#[tokio::test]
async fn catalog_status_readiness_and_doctor_use_legacy_internals() {
    let (application, _, _) = application(false, json!({"echo": "hello"}));
    let context = mounted_context(Confirmation::Missing, None);

    assert_eq!(application.catalog_snapshot().catalogs.len(), 1);
    assert_eq!(application.list_prompts().len(), 1);
    assert_eq!(
        application
            .get_prompt("quick_start", &context)
            .unwrap()
            .name,
        "quick_start"
    );
    assert_eq!(application.list_resources().len(), 1);
    assert_eq!(application.list_resource_templates().len(), 1);
    application.readiness().await.unwrap();
    assert_eq!(application.status().await.unwrap()["status"], "ok");
    let doctor = application.doctor().await;
    assert!(doctor.ready);
    assert!(doctor.problems.is_empty());
}

#[test]
fn prompt_discovery_is_unfiltered_and_scope_is_enforced_at_use_time() {
    let (application, _, _) = application(false, json!({"echo": "hello"}));
    let reader = mounted_context(Confirmation::Missing, None);
    let mut writer = mounted_context(Confirmation::Missing, None);
    writer.principal = Some(Principal::new("writer", ScopeSet::from([WRITE_SCOPE])));
    let mut context = mounted_context(Confirmation::Missing, None);
    context.principal = Some(Principal::anonymous());

    assert_eq!(application.list_prompts().len(), 1);
    assert_eq!(
        application
            .get_prompt("quick_start", &context)
            .unwrap_err()
            .code,
        "insufficient_scope"
    );
    assert!(application.get_prompt("quick_start", &reader).is_ok());
    assert!(application.get_prompt("quick_start", &writer).is_ok());
}

#[tokio::test]
async fn resource_discovery_is_unfiltered_and_scope_is_enforced_at_use_time() {
    let (application, _, _) = application(false, json!({}));
    let reader = mounted_context(Confirmation::Missing, None);
    let mut writer = mounted_context(Confirmation::Missing, None);
    writer.principal = Some(Principal::new("writer", ScopeSet::from([WRITE_SCOPE])));

    assert_eq!(application.list_resources().len(), 1);
    assert_eq!(application.list_resource_templates().len(), 1);

    let exact_uri = "soma://resources/recording/runbook.md";
    let reader_error = application
        .read_resource(
            crate::ReadResourceRequest {
                uri: exact_uri.to_owned(),
            },
            reader,
        )
        .await
        .unwrap_err();
    assert_eq!(reader_error.code, "insufficient_scope");

    let writer_error = application
        .read_resource(
            crate::ReadResourceRequest {
                uri: exact_uri.to_owned(),
            },
            writer,
        )
        .await
        .unwrap_err();
    assert_eq!(writer_error.code, "resource_read_not_supported");
}

#[test]
fn scaffold_validation_preserves_structured_application_error_details() {
    let (application, _, _) = application(false, json!({}));
    let error = application
        .scaffold_intent(ScaffoldIntentRequest {
            display_name: "Demo".to_owned(),
            crate_name: "Invalid Crate".to_owned(),
            binary_name: "demo".to_owned(),
            server_category: "upstream-client".to_owned(),
            env_prefix: "DEMO".to_owned(),
            auth_kind: "bearer".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 4000,
            mcp_transport: "stdio".to_owned(),
            mcp_primitives: "tools".to_owned(),
            deployment: "none".to_owned(),
            plugins: String::new(),
            publish_mcp: false,
            crawl_urls: String::new(),
            crawl_repos: String::new(),
            crawl_search_topics: String::new(),
        })
        .unwrap_err();

    assert!(error.is_validation());
    assert_eq!(error.code, "invalid_identifier");
    match *error.details {
        ApplicationErrorDetails::Service {
            field,
            expected_pattern,
            ..
        } => {
            assert_eq!(field.as_deref(), Some("crate_name"));
            assert_eq!(expected_pattern.as_deref(), Some("^[a-z][a-z0-9-]*$"));
        }
        details => panic!("expected service error details, got {details:?}"),
    }
}

#[test]
fn cli_catalog_queries_stay_behind_the_application_facade() {
    let (application, _, _) = application(true, json!({"echo": "hello"}));

    assert_eq!(application.resolve_cli_action("echo").unwrap(), "echo");
    assert!(application.action_requires_confirmation("echo"));
    assert_eq!(
        application.provider_for_action("echo").as_deref(),
        Some("recording")
    );
    assert_eq!(application.provider_validation_summary()["ok"], true);
    assert_eq!(
        application.provider_inspection_report()["providers"][0]["name"],
        "recording"
    );
}

#[test]
fn rest_catalog_queries_and_openapi_stay_behind_the_application_facade() {
    let (application, _, _) = application(false, json!({}));

    assert_eq!(
        application
            .resolve_rest_route("POST", "/v1/echo")
            .as_deref(),
        Some("echo")
    );
    assert!(
        application.openapi_document().unwrap()["paths"]
            .get("/v1/echo")
            .is_some()
    );
    let document = application.openapi_document().unwrap();
    assert_eq!(
        document["paths"]["/v1/echo"]["post"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["required"],
        json!(["output", "request_id", "progress"])
    );
}

#[test]
fn application_errors_redact_sensitive_diagnostics() {
    let port_error = ApplicationError::from(PortError::new(
        "engine_failed",
        "authorization: Bearer secret-value",
    ));
    let legacy_error = ApplicationError::legacy("status", "token=secret-value");
    let provider_error = ApplicationError::from(ProviderError::opaque_execution(
        "remote",
        "echo",
        "private-upstream-body",
    ));

    assert_eq!(port_error.message, "[redacted provider diagnostic]");
    assert!(!legacy_error.message.contains("secret-value"));
    assert!(!provider_error.message.contains("private-upstream-body"));
    assert_eq!(
        provider_error.private_diagnostics(),
        Some("private-upstream-body")
    );
}
