use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use soma_contracts::{actions::READ_SCOPE, config::SomaConfig, providers::ProviderCatalog};
use soma_domain::{
    AuthorizationMode, Confirmation, Principal, RequestId, ScopeSet, Surface, TraceContext,
};
use soma_service::{
    provider_registry::Provider, ProviderCall, ProviderOutput, ProviderRegistry, SomaClient,
    SomaService, StaticRustProvider,
};

use super::{
    CodeModeExecuteRequest, ExecuteActionRequest, GatewayExecuteRequest, GatewayReloadRequest,
    OpenApiExecuteRequest, SomaApplication,
};
use crate::{
    ApplicationError, ApplicationPorts, CodeModePort, ExecutionContext, GatewayPort, OpenApiPort,
    PortError,
};

struct RecordingProvider {
    catalog: ProviderCatalog,
    output: Value,
    calls: Mutex<Vec<ProviderCall>>,
}

#[async_trait]
impl Provider for RecordingProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(
        &self,
        call: ProviderCall,
    ) -> Result<ProviderOutput, soma_service::ProviderError> {
        self.calls.lock().unwrap().push(call);
        Ok(ProviderOutput::json(self.output.clone()))
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
    catalog.tools.retain(|tool| tool.name == "echo");
    catalog.tools[0].destructive = destructive;
    catalog.prompts[0].template = Some("Run {{action}}".to_owned());
    catalog.prompts[0].scope = Some(READ_SCOPE.to_owned());
    let provider = Arc::new(RecordingProvider {
        catalog,
        output,
        calls: Mutex::new(Vec::new()),
    });
    let registry = ProviderRegistry::new(vec![provider.clone()]).unwrap();
    let service = SomaService::new(SomaClient::new(&SomaConfig::default()).unwrap());
    let engines = Arc::new(RecordingEngines::default());
    let ports = ApplicationPorts {
        gateway: engines.clone(),
        codemode: engines.clone(),
        openapi: engines.clone(),
    };
    (
        SomaApplication::new(Arc::new(service), Arc::new(registry), ports),
        provider,
        engines,
    )
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
    assert_eq!(calls[0].surface, soma_service::ProviderSurface::Rest);
    assert_eq!(
        calls[0].limits,
        soma_service::ProviderRequestLimits::default()
    );
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
    assert_eq!(application.list_prompts(&context).len(), 1);
    assert_eq!(
        application
            .get_prompt("quick_start", &context)
            .unwrap()
            .name,
        "quick_start"
    );
    assert!(application.list_resources(&context).is_empty());
    application.readiness().await.unwrap();
    assert_eq!(application.status().await.unwrap()["status"], "ok");
    let doctor = application.doctor().await;
    assert!(doctor.ready);
    assert!(doctor.problems.is_empty());
}

#[test]
fn prompt_discovery_filters_mounted_scopes() {
    let (application, _, _) = application(false, json!({"echo": "hello"}));
    let mut context = mounted_context(Confirmation::Missing, None);
    context.principal = Some(Principal::anonymous());

    assert!(application.list_prompts(&context).is_empty());
    assert_eq!(
        application
            .get_prompt("quick_start", &context)
            .unwrap_err()
            .code,
        "prompt_not_found"
    );
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
    assert!(application.openapi_document().unwrap()["paths"]
        .get("/v1/echo")
        .is_some());
}

#[test]
fn application_errors_redact_sensitive_diagnostics() {
    let port_error = ApplicationError::from(PortError::new(
        "engine_failed",
        "authorization: Bearer secret-value",
    ));
    let legacy_error = ApplicationError::legacy("status", "token=secret-value");

    assert_eq!(port_error.message, "[redacted provider diagnostic]");
    assert!(!legacy_error.message.contains("secret-value"));
}
