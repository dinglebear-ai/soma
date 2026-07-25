use std::sync::Arc;

use serde_json::Value;
use soma_domain::{
    scopes::{READ_SCOPE, WRITE_SCOPE},
    token_limit::MAX_RESPONSE_BYTES,
    AuthorizationMode, Principal, Surface,
};
use soma_provider_core::{ProviderPrompt, ProviderResource};

use crate::{
    ApplicationError, ApplicationPorts, CatalogSnapshot, CodeModeExecuteRequest, DoctorReport,
    ElicitedName, ElicitedNameOutcome, ExecuteActionRequest, ExecuteActionResponse,
    ExecutionContext, GatewayExecuteRequest, GatewayPromptRoute, GatewayReloadRequest,
    GatewayResourceRoute, GatewayRouteScope, GatewayToolRoute, OpenApiExecuteRequest,
    OperationResponse, ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRegistry,
    ProviderRequestLimits, ProviderSurface, ReadResourceRequest, ResourceContent,
    ResourceReadOutput, ResourceTemplateSpec, ScaffoldIntent, ScaffoldIntentRequest, SomaService,
};

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

/// Shared use-case facade every surface (MCP, REST, CLI) calls into.
///
/// Wraps the legacy [`SomaService`] and [`ProviderRegistry`] plus the outbound
/// [`ApplicationPorts`] (gateway, code mode, OpenAPI), exposing one method per
/// application operation.
pub struct SomaApplication {
    legacy_service: Arc<SomaService>,
    legacy_registry: Arc<ProviderRegistry>,
    ports: ApplicationPorts,
}

impl SomaApplication {
    /// Assemble the facade from its service, provider registry, and outbound ports.
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

    /// Dispatch an action through the provider registry and return its output.
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

    /// Build the greeting for the elicited-name demo from the collected outcome.
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

    /// Normalize elicited scaffold requirements into the JSON handoff contract.
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

    /// Return a snapshot of the currently loaded provider catalog.
    pub fn catalog_snapshot(&self) -> CatalogSnapshot {
        catalog_snapshot(self.legacy_registry.snapshot().as_ref())
    }

    /// Resolve a CLI command name to its backing action, or a not-found error.
    pub fn resolve_cli_action(&self, command: &str) -> Result<String, ApplicationError> {
        self.legacy_registry
            .snapshot()
            .cli_action(command)
            .map(ToOwned::to_owned)
            .ok_or_else(|| ApplicationError::not_found("CLI command", command))
    }

    /// Report whether the action is destructive and requires explicit confirmation.
    pub fn action_requires_confirmation(&self, action: &str) -> bool {
        self.legacy_registry
            .snapshot()
            .action_requires_confirmation(action)
    }

    /// Return the name of the provider that owns the given action, if any.
    pub fn provider_for_action(&self, action: &str) -> Option<String> {
        self.legacy_registry
            .snapshot()
            .provider_for_action(action)
            .map(ToOwned::to_owned)
    }

    /// Return the provider validation summary for the loaded catalog.
    pub fn provider_validation_summary(&self) -> Value {
        self.legacy_registry.snapshot().validation_summary()
    }

    /// Return the detailed provider inspection report for the loaded catalog.
    pub fn provider_inspection_report(&self) -> Value {
        self.legacy_registry.snapshot().inspection_report()
    }

    /// Resolve an HTTP method and path to its backing action, if a route matches.
    pub fn resolve_rest_route(&self, method: &str, path: &str) -> Option<String> {
        self.legacy_registry
            .snapshot()
            .route_action(method, path)
            .map(ToOwned::to_owned)
    }

    /// Return the runtime OpenAPI document assembled from the provider catalog.
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

    /// Reload file-backed providers and return the resulting catalog snapshot.
    pub fn refresh_providers(&self) -> Result<CatalogSnapshot, ApplicationError> {
        self.refresh_providers_in_place()?;
        Ok(self.catalog_snapshot())
    }

    /// Reload file-backed providers without returning a snapshot.
    pub fn refresh_providers_in_place(&self) -> Result<(), ApplicationError> {
        self.legacy_registry
            .refresh_file_providers()
            .map(|_| ())
            .map_err(|error| {
                let diagnostic = crate::provider_errors::redact_public(&error.to_string());
                ApplicationError::new(
                    "provider_refresh_failed",
                    format!("provider refresh failed: {diagnostic}"),
                    false,
                    "Fix invalid provider files and retry.",
                )
            })
    }

    /// Read a provider resource by URI and return its text or blob content.
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

    /// List the exact (non-templated) provider resources in the catalog.
    pub fn list_resources(&self) -> Vec<ProviderResource> {
        self.legacy_registry
            .snapshot()
            .exact_resources()
            .cloned()
            .collect()
    }

    /// List the dynamic (URI-templated) provider resource templates in the catalog.
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

    /// List the servable provider prompts in the catalog.
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

    /// Fetch a servable prompt by name, enforcing scope visibility.
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

    /// Query the MCP gateway status via the gateway port.
    pub async fn gateway_status(
        &self,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.gateway.status(&context).await?;
        operation_response(output, &context)
    }

    /// Reload the MCP gateway configuration via the gateway port.
    pub async fn gateway_reload(
        &self,
        request: GatewayReloadRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.gateway.reload(request, &context).await?;
        operation_response(output, &context)
    }

    /// Execute a gateway operation via the gateway port.
    pub async fn gateway_execute(
        &self,
        request: GatewayExecuteRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.gateway.execute(request, &context).await?;
        operation_response(output, &context)
    }

    /// List MCP tools exposed through the gateway, optionally filtered by scope.
    pub async fn gateway_mcp_tools(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayToolRoute>, ApplicationError> {
        Ok(self.ports.gateway.list_mcp_tools(scope, context).await?)
    }

    /// Call an MCP tool through the gateway, returning its result if routed.
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

    /// List MCP resources exposed through the gateway, optionally filtered by scope.
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

    /// Read an MCP resource through the gateway, returning its content if routed.
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

    /// List MCP prompts exposed through the gateway, optionally filtered by scope.
    pub async fn gateway_mcp_prompts(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayPromptRoute>, ApplicationError> {
        Ok(self.ports.gateway.list_mcp_prompts(scope, context).await?)
    }

    /// Fetch an MCP prompt through the gateway, returning its content if routed.
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

    /// Execute a Code Mode request via the code mode port.
    pub async fn codemode_execute(
        &self,
        request: CodeModeExecuteRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.codemode.execute(request, &context).await?;
        operation_response(output, &context)
    }

    /// Execute an OpenAPI-backed request via the OpenAPI port.
    pub async fn openapi_execute(
        &self,
        request: OpenApiExecuteRequest,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApplicationError> {
        let output = self.ports.openapi.execute(request, &context).await?;
        operation_response(output, &context)
    }

    /// Return the upstream service status.
    pub async fn status(&self) -> Result<Value, ApplicationError> {
        self.legacy_service
            .status()
            .await
            .map_err(|error| ApplicationError::legacy("status", error))
    }

    /// Probe upstream readiness; `Ok(())` when the dependency is reachable.
    pub async fn readiness(&self) -> Result<(), ApplicationError> {
        self.legacy_service
            .ready()
            .await
            .map_err(|error| ApplicationError::legacy("readiness", error))
    }

    /// Run readiness and status probes and collect them into a diagnostic report.
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

fn catalog_snapshot(snapshot: &crate::RegistrySnapshot) -> CatalogSnapshot {
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
