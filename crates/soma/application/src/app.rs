use std::sync::Arc;

use serde_json::Value;
use soma_domain::{
    scopes::{READ_SCOPE, WRITE_SCOPE},
    token_limit::MAX_RESPONSE_BYTES,
    AuthorizationMode, Principal, Surface,
};
use soma_provider_core::{ProviderPrompt, ProviderResource};
use soma_service::{
    ElicitedNameOutcome, ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRegistry,
    ProviderRequestLimits, ProviderSurface, ResourceReadOutput, ScaffoldIntent, SomaService,
};

use crate::{
    ApplicationError, ApplicationPorts, CatalogSnapshot, CodeModeExecuteRequest, DoctorReport,
    ElicitedName, ExecuteActionRequest, ExecuteActionResponse, ExecutionContext,
    GatewayExecuteRequest, GatewayPromptRoute, GatewayReloadRequest, GatewayResourceRoute,
    GatewayRouteScope, GatewayToolRoute, OpenApiExecuteRequest, OperationResponse,
    ReadResourceRequest, ResourceContent, ResourceTemplateSpec, ScaffoldIntentRequest,
};

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

pub struct SomaApplication {
    legacy_service: Arc<SomaService>,
    legacy_registry: Arc<ProviderRegistry>,
    ports: ApplicationPorts,
}

impl SomaApplication {
    pub fn new(
        legacy_service: Arc<SomaService>,
        legacy_registry: Arc<ProviderRegistry>,
        ports: ApplicationPorts,
    ) -> Self {
        Self {
            legacy_service,
            legacy_registry,
            ports,
        }
    }

    pub async fn execute_action(
        &self,
        request: ExecuteActionRequest,
        context: ExecutionContext,
    ) -> Result<ExecuteActionResponse, ApplicationError> {
        let limits = ProviderRequestLimits {
            max_response_bytes: context
                .response_limit
                .unwrap_or(ProviderRequestLimits::default().max_response_bytes),
            ..ProviderRequestLimits::default()
        };
        let call = ProviderCall {
            provider: String::new(),
            action: request.action,
            params: request.params,
            principal: provider_principal(context.principal.as_ref()),
            auth_mode: provider_auth_mode(context.authorization_mode),
            surface: provider_surface(context.surface),
            destructive_confirmed: context.destructive_confirmation.is_confirmed(),
            limits,
            snapshot_id: String::new(),
        };
        let output = self.legacy_registry.dispatch(call).await?;
        Ok(ExecuteActionResponse {
            output: output.value,
            request_id: context.request_id.as_str().to_owned(),
        })
    }

    pub fn elicited_name_greeting(&self, outcome: ElicitedName) -> Value {
        match outcome {
            ElicitedName::Accepted(name) => self
                .legacy_service
                .elicited_name_greeting(ElicitedNameOutcome::Accepted(&name)),
            ElicitedName::NoInput => self
                .legacy_service
                .elicited_name_greeting(ElicitedNameOutcome::NoInput),
            ElicitedName::Declined => self
                .legacy_service
                .elicited_name_greeting(ElicitedNameOutcome::Declined),
            ElicitedName::Cancelled => self
                .legacy_service
                .elicited_name_greeting(ElicitedNameOutcome::Cancelled),
            ElicitedName::Unsupported => self
                .legacy_service
                .elicited_name_greeting(ElicitedNameOutcome::Unsupported),
        }
    }

    pub fn scaffold_intent(
        &self,
        request: ScaffoldIntentRequest,
    ) -> Result<Value, ApplicationError> {
        self.legacy_service
            .scaffold_intent(ScaffoldIntent {
                display_name: request.display_name,
                crate_name: request.crate_name,
                binary_name: request.binary_name,
                server_category: request.server_category,
                env_prefix: request.env_prefix,
                auth_kind: request.auth_kind,
                host: request.host,
                port: request.port,
                mcp_transport: request.mcp_transport,
                mcp_primitives: request.mcp_primitives,
                deployment: request.deployment,
                plugins: request.plugins,
                publish_mcp: request.publish_mcp,
                crawl_urls: request.crawl_urls,
                crawl_repos: request.crawl_repos,
                crawl_search_topics: request.crawl_search_topics,
            })
            .map_err(|error| ApplicationError::service(&error))
    }

    pub fn catalog_snapshot(&self) -> CatalogSnapshot {
        catalog_snapshot(self.legacy_registry.snapshot().as_ref())
    }

    pub fn resolve_cli_action(&self, command: &str) -> Result<String, ApplicationError> {
        self.legacy_registry
            .snapshot()
            .cli_action(command)
            .map(ToOwned::to_owned)
            .ok_or_else(|| ApplicationError::not_found("CLI command", command))
    }

    pub fn action_requires_confirmation(&self, action: &str) -> bool {
        self.legacy_registry
            .snapshot()
            .action_requires_confirmation(action)
    }

    pub fn provider_for_action(&self, action: &str) -> Option<String> {
        self.legacy_registry
            .snapshot()
            .provider_for_action(action)
            .map(ToOwned::to_owned)
    }

    pub fn provider_validation_summary(&self) -> Value {
        self.legacy_registry.snapshot().validation_summary()
    }

    pub fn provider_inspection_report(&self) -> Value {
        self.legacy_registry.snapshot().inspection_report()
    }

    pub fn resolve_rest_route(&self, method: &str, path: &str) -> Option<String> {
        self.legacy_registry
            .snapshot()
            .route_action(method, path)
            .map(ToOwned::to_owned)
    }

    pub fn openapi_document(&self) -> Result<Value, ApplicationError> {
        serde_json::from_slice(&self.legacy_registry.snapshot().cached_openapi_bytes).map_err(
            |error| {
                ApplicationError::new(
                    "openapi_unavailable",
                    format!("runtime OpenAPI document is unavailable: {error}"),
                    false,
                    "Refresh the provider catalog and retry.",
                )
            },
        )
    }

    pub fn refresh_providers(&self) -> Result<CatalogSnapshot, ApplicationError> {
        self.refresh_providers_in_place()?;
        Ok(self.catalog_snapshot())
    }

    pub fn refresh_providers_in_place(&self) -> Result<(), ApplicationError> {
        self.legacy_registry
            .refresh_file_providers()
            .map(|_| ())
            .map_err(|error| {
                let diagnostic = soma_service::provider_errors::redact_public(&error.to_string());
                ApplicationError::new(
                    "provider_refresh_failed",
                    format!("provider refresh failed: {diagnostic}"),
                    false,
                    "Fix invalid provider files and retry.",
                )
            })
    }

    pub async fn read_resource(
        &self,
        request: ReadResourceRequest,
        context: ExecutionContext,
    ) -> Result<ResourceContent, ApplicationError> {
        let output = self
            .legacy_registry
            .read_resource(
                &request.uri,
                &provider_principal(context.principal.as_ref()),
                provider_auth_mode(context.authorization_mode),
            )
            .await?;
        Ok(match output {
            ResourceReadOutput::Text { text, mime_type } => {
                ResourceContent::Text { text, mime_type }
            }
            ResourceReadOutput::Blob {
                blob_base64,
                mime_type,
            } => ResourceContent::Blob {
                blob_base64,
                mime_type,
            },
        })
    }

    pub fn list_resources(&self) -> Vec<ProviderResource> {
        self.legacy_registry
            .snapshot()
            .exact_resources()
            .cloned()
            .collect()
    }

    pub fn list_resource_templates(&self) -> Vec<ResourceTemplateSpec> {
        self.legacy_registry
            .snapshot()
            .dynamic_resource_templates()
            .iter()
            .map(|(_, template)| template)
            .map(|template| ResourceTemplateSpec {
                uri_template: template.uri_template(),
                name: template.name.clone(),
                description: template.description.clone(),
                mime_type: template.mime_type.clone(),
            })
            .collect()
    }

    pub fn list_prompts(&self) -> Vec<ProviderPrompt> {
        self.legacy_registry
            .snapshot()
            .catalogs
            .iter()
            .flat_map(|catalog| catalog.prompts.iter())
            .filter(|prompt| prompt_is_servable(prompt))
            .cloned()
            .collect()
    }

    pub fn get_prompt(
        &self,
        name: &str,
        context: &ExecutionContext,
    ) -> Result<ProviderPrompt, ApplicationError> {
        let prompt = self
            .legacy_registry
            .snapshot()
            .catalogs
            .iter()
            .flat_map(|catalog| catalog.prompts.iter())
            .filter(|prompt| prompt_is_servable(prompt))
            .find(|prompt| prompt.name == name)
            .cloned()
            .ok_or_else(|| ApplicationError::not_found("prompt", name))?;
        if !scope_visible(prompt.scope.as_deref(), context) {
            let required = prompt.scope.as_deref().unwrap_or_default();
            return Err(ApplicationError::new(
                "insufficient_scope",
                format!("prompt `{name}` requires scope `{required}`"),
                false,
                "Authenticate with a token that includes the required scope.",
            ));
        }
        Ok(prompt)
    }

    pub async fn gateway_status(
        &self,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.gateway.status(&context).await?;
        operation_response(output, &context)
    }

    pub async fn gateway_reload(
        &self,
        request: GatewayReloadRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.gateway.reload(request, &context).await?;
        operation_response(output, &context)
    }

    pub async fn gateway_execute(
        &self,
        request: GatewayExecuteRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.gateway.execute(request, &context).await?;
        operation_response(output, &context)
    }

    pub async fn gateway_mcp_tools(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayToolRoute>, ApplicationError> {
        Ok(self.ports.gateway.list_mcp_tools(scope, context).await?)
    }

    pub async fn gateway_call_mcp_tool(
        &self,
        name: &str,
        params: Value,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, ApplicationError> {
        Ok(self
            .ports
            .gateway
            .call_mcp_tool(name, params, scope, context)
            .await?)
    }

    pub async fn gateway_mcp_resources(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayResourceRoute>, ApplicationError> {
        Ok(self
            .ports
            .gateway
            .list_mcp_resources(scope, context)
            .await?)
    }

    pub async fn gateway_read_mcp_resource(
        &self,
        uri: &str,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, ApplicationError> {
        Ok(self
            .ports
            .gateway
            .read_mcp_resource(uri, scope, context)
            .await?)
    }

    pub async fn gateway_mcp_prompts(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayPromptRoute>, ApplicationError> {
        Ok(self.ports.gateway.list_mcp_prompts(scope, context).await?)
    }

    pub async fn gateway_get_mcp_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, ApplicationError> {
        Ok(self
            .ports
            .gateway
            .get_mcp_prompt(name, arguments, scope, context)
            .await?)
    }

    pub async fn codemode_execute(
        &self,
        request: CodeModeExecuteRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.codemode.execute(request, &context).await?;
        operation_response(output, &context)
    }

    pub async fn openapi_execute(
        &self,
        request: OpenApiExecuteRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.openapi.execute(request, &context).await?;
        operation_response(output, &context)
    }

    pub async fn status(&self) -> Result<Value, ApplicationError> {
        self.legacy_service
            .status()
            .await
            .map_err(|error| ApplicationError::legacy("status", error))
    }

    pub async fn readiness(&self) -> Result<(), ApplicationError> {
        self.legacy_service
            .ready()
            .await
            .map_err(|error| ApplicationError::legacy("readiness", error))
    }

    pub async fn doctor(&self) -> DoctorReport {
        let mut problems = Vec::new();
        let ready = match self.readiness().await {
            Ok(()) => true,
            Err(error) => {
                problems.push(error.to_string());
                false
            }
        };
        let status = match self.status().await {
            Ok(status) => Some(status),
            Err(error) => {
                problems.push(error.to_string());
                None
            }
        };
        DoctorReport {
            ready,
            status,
            problems,
        }
    }
}

fn catalog_snapshot(snapshot: &soma_service::RegistrySnapshot) -> CatalogSnapshot {
    CatalogSnapshot {
        id: snapshot.id.clone(),
        fingerprint: snapshot.fingerprint.clone(),
        catalogs: snapshot.catalogs.clone(),
    }
}

fn operation_response(
    output: Value,
    context: &ExecutionContext,
) -> Result<OperationResponse, ApplicationError> {
    let maximum = context.response_limit.unwrap_or(MAX_RESPONSE_BYTES);
    let actual = serde_json::to_vec(&output)
        .map_err(|error| ApplicationError::legacy("response serialization", error))?
        .len();
    if actual > maximum {
        return Err(ApplicationError::new(
            "response_too_large",
            format!("response is {actual} bytes; maximum is {maximum}"),
            false,
            "Increase the response limit or request a smaller result.",
        ));
    }
    Ok(OperationResponse {
        output,
        request_id: context.request_id.as_str().to_owned(),
    })
}

fn provider_principal(principal: Option<&Principal>) -> ProviderPrincipal {
    principal.map_or_else(ProviderPrincipal::anonymous, |principal| {
        ProviderPrincipal {
            subject: principal.subject.clone(),
            scopes: principal.scopes.to_vec(),
        }
    })
}

fn provider_auth_mode(mode: AuthorizationMode) -> ProviderAuthMode {
    match mode {
        AuthorizationMode::LoopbackDev => ProviderAuthMode::LoopbackDev,
        AuthorizationMode::TrustedGateway => ProviderAuthMode::TrustedGateway,
        AuthorizationMode::Mounted => ProviderAuthMode::Mounted,
    }
}

fn provider_surface(surface: Surface) -> ProviderSurface {
    match surface {
        Surface::Mcp => ProviderSurface::Mcp,
        Surface::Rest => ProviderSurface::Rest,
        Surface::Cli => ProviderSurface::Cli,
        Surface::Palette => ProviderSurface::Palette,
    }
}

fn scope_visible(required: Option<&str>, context: &ExecutionContext) -> bool {
    if !matches!(context.authorization_mode, AuthorizationMode::Mounted) {
        return true;
    }
    let Some(required) = required else {
        return true;
    };
    context.principal.as_ref().is_some_and(|principal| {
        principal.scopes.contains(required)
            || (required == READ_SCOPE && principal.scopes.contains(WRITE_SCOPE))
    })
}

fn prompt_is_servable(prompt: &ProviderPrompt) -> bool {
    prompt.template.is_some()
        && prompt
            .mcp
            .as_ref()
            .map(|metadata| metadata.enabled)
            .unwrap_or(true)
}
