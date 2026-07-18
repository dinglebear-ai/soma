use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    CodeModeExecuteRequest, ExecutionContext, GatewayExecuteRequest, GatewayPromptRoute,
    GatewayReloadRequest, GatewayResourceRoute, GatewayRouteScope, GatewayToolRoute,
    OpenApiExecuteRequest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub remediation: String,
}

impl PortError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            remediation: "Check the engine configuration and retry.".to_owned(),
        }
    }
}

#[async_trait]
pub trait GatewayPort: Send + Sync {
    async fn status(&self, context: &ExecutionContext) -> Result<Value, PortError>;
    async fn reload(
        &self,
        request: GatewayReloadRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;
    async fn execute(
        &self,
        request: GatewayExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;

    async fn list_mcp_tools(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayToolRoute>, PortError>;

    async fn call_mcp_tool(
        &self,
        name: &str,
        params: Value,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError>;

    async fn list_mcp_resources(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayResourceRoute>, PortError>;

    async fn read_mcp_resource(
        &self,
        uri: &str,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError>;

    async fn list_mcp_prompts(
        &self,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Vec<GatewayPromptRoute>, PortError>;

    async fn get_mcp_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError>;
}

#[async_trait]
pub trait CodeModePort: Send + Sync {
    async fn execute(
        &self,
        request: CodeModeExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;
}

#[async_trait]
pub trait OpenApiPort: Send + Sync {
    async fn execute(
        &self,
        request: OpenApiExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError>;
}

pub struct ApplicationPorts {
    pub gateway: Arc<dyn GatewayPort>,
    pub codemode: Arc<dyn CodeModePort>,
    pub openapi: Arc<dyn OpenApiPort>,
}

impl ApplicationPorts {
    pub fn unavailable() -> Self {
        let port = Arc::new(UnavailableEnginePort);
        Self {
            gateway: port.clone(),
            codemode: port.clone(),
            openapi: port,
        }
    }

    pub fn with_gateway(mut self, gateway: Arc<dyn GatewayPort>) -> Self {
        self.gateway = gateway;
        self
    }

    pub fn with_codemode(mut self, codemode: Arc<dyn CodeModePort>) -> Self {
        self.codemode = codemode;
        self
    }

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
