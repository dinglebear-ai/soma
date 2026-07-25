use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    CodeModeExecuteRequest, ExecutionContext, GatewayExecuteRequest, GatewayPromptRoute,
    GatewayReloadRequest, GatewayResourceRoute, GatewayRouteScope, GatewayToolRoute,
    OpenApiExecuteRequest,
};

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

/// Bundle of the engine ports the application depends on.
pub struct ApplicationPorts {
    /// MCP gateway engine port.
    pub gateway: Arc<dyn GatewayPort>,
    /// Code Mode engine port.
    pub codemode: Arc<dyn CodeModePort>,
    /// OpenAPI engine port.
    pub openapi: Arc<dyn OpenApiPort>,
}

impl ApplicationPorts {
    /// Builds a port bundle where every engine reports itself as unavailable.
    pub fn unavailable() -> Self {
        let port = Arc::new(UnavailableEnginePort);
        Self {
            gateway: port.clone(),
            codemode: port.clone(),
            openapi: port,
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
