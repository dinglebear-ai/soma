use std::{collections::BTreeMap, path::Path, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    CodeModeExecuteRequest, ExecutionContext, GatewayExecuteRequest, GatewayMcpOutcome,
    GatewayMcpRoundTrip, GatewayPromptRoute, GatewayReloadRequest, GatewayResourceRoute,
    GatewayRouteScope, GatewayToolRoute, OpenApiExecuteRequest,
};
use soma_provider_adapters::python::materializer::PreparedPythonEnvironment;

/// Error returned by an engine port when an operation cannot be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError {
    /// Stable, machine-readable error code.
    pub code: String,
    /// Human-readable description of the failure.
    pub message: String,
    /// Whether retrying the operation might succeed.
    pub retryable: bool,
    /// Suggested remediation the caller can act on.
    pub remediation: String,
}

impl PortError {
    /// Builds a `PortError` from a code and message, defaulting to
    /// non-retryable with a generic remediation hint.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            remediation: "Check the engine configuration and retry.".to_owned(),
        }
    }
}

/// Port to the MCP gateway engine: status, reload, execution, and
/// tool/resource/prompt routing.
#[async_trait]
pub trait GatewayPort: Send + Sync {
    /// Returns the gateway's current status snapshot.
    async fn status(&self, context: &ExecutionContext) -> Result<Value, PortError>;
    /// Reloads the gateway configuration.
    async fn reload(
        &self,
        request: GatewayReloadRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;
    /// Executes a gateway operation.
    async fn execute(
        &self,
        request: GatewayExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;

    /// Lists MCP tool routes exposed through the gateway, optionally scoped.
    async fn list_mcp_tools(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayToolRoute>, PortError>;

    /// Calls an MCP tool by name through the gateway.
    async fn call_mcp_tool(
        &self,
        name: &str,
        params: Value,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError>;

    /// Calls an MCP tool once, preserving modern multi-round-trip and task outcomes.
    async fn call_mcp_tool_once(
        &self,
        name: &str,
        params: Value,
        round_trip: GatewayMcpRoundTrip,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<GatewayMcpOutcome>, PortError> {
        let _ = round_trip;
        self.call_mcp_tool(name, params, scope, context)
            .await
            .map(|result| {
                result.map(|value| {
                    GatewayMcpOutcome::Complete(serde_json::json!({
                        "resultType": "complete",
                        "content": [],
                        "structuredContent": value
                    }))
                })
            })
    }

    /// Gets the latest state of a routed MCP task.
    async fn get_mcp_task(
        &self,
        task_id: &str,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        let _ = (task_id, context);
        Err(PortError::new(
            "tasks_unsupported",
            "the configured gateway does not support MCP tasks",
        ))
    }

    /// Supplies follow-up input to a routed MCP task.
    async fn update_mcp_task(
        &self,
        task_id: &str,
        input_responses: BTreeMap<String, Value>,
        context: &ExecutionContext,
    ) -> Result<(), PortError> {
        let _ = (task_id, input_responses, context);
        Err(PortError::new(
            "tasks_unsupported",
            "the configured gateway does not support MCP tasks",
        ))
    }

    /// Requests cancellation of a routed MCP task.
    async fn cancel_mcp_task(
        &self,
        task_id: &str,
        context: &ExecutionContext,
    ) -> Result<(), PortError> {
        let _ = (task_id, context);
        Err(PortError::new(
            "tasks_unsupported",
            "the configured gateway does not support MCP tasks",
        ))
    }

    /// Lists MCP resource routes exposed through the gateway, optionally scoped.
    async fn list_mcp_resources(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayResourceRoute>, PortError>;

    /// Reads an MCP resource by URI through the gateway.
    async fn read_mcp_resource(
        &self,
        uri: &str,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError>;

    /// Reads an MCP resource once, preserving a possible input-required outcome.
    async fn read_mcp_resource_once(
        &self,
        uri: &str,
        round_trip: GatewayMcpRoundTrip,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<GatewayMcpOutcome>, PortError> {
        let _ = round_trip;
        self.read_mcp_resource(uri, scope, context)
            .await
            .map(|result| result.map(GatewayMcpOutcome::Complete))
    }

    /// Lists MCP prompt routes exposed through the gateway, optionally scoped.
    async fn list_mcp_prompts(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayPromptRoute>, PortError>;

    /// Gets an MCP prompt by name, with optional arguments, through the gateway.
    async fn get_mcp_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError>;
    /// Gets an MCP prompt once, preserving a possible input-required outcome.
    async fn get_mcp_prompt_once(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        round_trip: GatewayMcpRoundTrip,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<GatewayMcpOutcome>, PortError> {
        let _ = round_trip;
        self.get_mcp_prompt(name, arguments, scope, context)
            .await
            .map(|result| result.map(GatewayMcpOutcome::Complete))
    }
}

/// Port to the Code Mode engine that runs sandboxed JavaScript against the
/// tool catalog.
#[async_trait]
pub trait CodeModePort: Send + Sync {
    /// Executes a Code Mode request.
    async fn execute(
        &self,
        request: CodeModeExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;
}

/// Port to the OpenAPI engine that dispatches requests against described APIs.
#[async_trait]
pub trait OpenApiPort: Send + Sync {
    /// Executes an OpenAPI request.
    async fn execute(
        &self,
        request: OpenApiExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;
}

/// Result of preparing an immutable environment update before registry activation.
pub struct PythonEnvironmentUpdateCandidate {
    /// Operator-facing update report.
    pub report: Value,
    /// Exact candidate that the registry must validate and activate atomically.
    pub candidate: PreparedPythonEnvironment,
}

/// Operator control plane for production-managed Python environments.
pub trait PythonEnvironmentPort: Send + Sync {
    /// Inventories every managed cache entry without executing provider code.
    fn status(&self) -> Result<Value, PortError>;
    /// Plans or applies a bounded prune of stale non-ready entries.
    fn prune(
        &self,
        stale_before_unix_seconds: u64,
        max_entries: usize,
        apply: bool,
    ) -> Result<Value, PortError>;
    /// Repairs the exact environment planned for one managed provider.
    fn repair(&self, provider_path: &Path) -> Result<Value, PortError>;
    /// Prepares an immutable candidate for later atomic registry activation.
    fn update(&self, provider_path: &Path) -> Result<PythonEnvironmentUpdateCandidate, PortError>;
}

/// Bundle of the engine ports the application depends on.
pub struct ApplicationPorts {
    /// MCP gateway engine port.
    pub gateway: Arc<dyn GatewayPort>,
    /// Code Mode engine port.
    pub codemode: Arc<dyn CodeModePort>,
    /// OpenAPI engine port.
    pub openapi: Arc<dyn OpenApiPort>,
    /// Immutable Python environment operator port.
    pub python_environment: Arc<dyn PythonEnvironmentPort>,
}

impl ApplicationPorts {
    /// Builds a port bundle where every engine reports itself as unavailable.
    pub fn unavailable() -> Self {
        let port = Arc::new(UnavailableEnginePort);
        Self {
            gateway: port.clone(),
            codemode: port.clone(),
            openapi: port.clone(),
            python_environment: port,
        }
    }

    /// Replaces the gateway port and returns the updated bundle.
    pub fn with_gateway(mut self, gateway: Arc<dyn GatewayPort>) -> Self {
        self.gateway = gateway;
        self
    }

    /// Replaces the Code Mode port and returns the updated bundle.
    pub fn with_codemode(mut self, codemode: Arc<dyn CodeModePort>) -> Self {
        self.codemode = codemode;
        self
    }

    /// Replaces the OpenAPI port and returns the updated bundle.
    pub fn with_openapi(mut self, openapi: Arc<dyn OpenApiPort>) -> Self {
        self.openapi = openapi;
        self
    }

    /// Replaces the Python environment operator port and returns the updated bundle.
    pub fn with_python_environment(
        mut self,
        python_environment: Arc<dyn PythonEnvironmentPort>,
    ) -> Self {
        self.python_environment = python_environment;
        self
    }
}

struct UnavailableEnginePort;

impl UnavailableEnginePort {
    fn error(engine: &str) -> PortError {
        PortError::new(
            "engine_unavailable",
            format!("{engine} is not configured for this application instance"),
        )
    }
}

#[async_trait]
impl GatewayPort for UnavailableEnginePort {
    async fn status(&self, _context: &ExecutionContext) -> Result<Value, PortError> {
        Err(Self::error("gateway"))
    }

    async fn reload(
        &self,
        _request: GatewayReloadRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Err(Self::error("gateway"))
    }

    async fn execute(
        &self,
        _request: GatewayExecuteRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Err(Self::error("gateway"))
    }

    async fn list_mcp_tools(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayToolRoute>, PortError> {
        Err(Self::error("gateway"))
    }

    async fn call_mcp_tool(
        &self,
        _name: &str,
        _params: Value,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Err(Self::error("gateway"))
    }

    async fn list_mcp_resources(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayResourceRoute>, PortError> {
        Err(Self::error("gateway"))
    }

    async fn read_mcp_resource(
        &self,
        _uri: &str,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Err(Self::error("gateway"))
    }

    async fn list_mcp_prompts(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayPromptRoute>, PortError> {
        Err(Self::error("gateway"))
    }

    async fn get_mcp_prompt(
        &self,
        _name: &str,
        _arguments: Option<serde_json::Map<String, Value>>,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Err(Self::error("gateway"))
    }
}

#[async_trait]
impl CodeModePort for UnavailableEnginePort {
    async fn execute(
        &self,
        _request: CodeModeExecuteRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Err(Self::error("Code Mode"))
    }
}

#[async_trait]
impl OpenApiPort for UnavailableEnginePort {
    async fn execute(
        &self,
        _request: OpenApiExecuteRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Err(Self::error("OpenAPI"))
    }
}

impl PythonEnvironmentPort for UnavailableEnginePort {
    fn status(&self) -> Result<Value, PortError> {
        Err(Self::error("Python environment lifecycle"))
    }

    fn prune(
        &self,
        _stale_before_unix_seconds: u64,
        _max_entries: usize,
        _apply: bool,
    ) -> Result<Value, PortError> {
        Err(Self::error("Python environment lifecycle"))
    }

    fn repair(&self, _provider_path: &Path) -> Result<Value, PortError> {
        Err(Self::error("Python environment lifecycle"))
    }

    fn update(&self, _provider_path: &Path) -> Result<PythonEnvironmentUpdateCandidate, PortError> {
        Err(Self::error("Python environment lifecycle"))
    }
}
