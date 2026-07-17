mod app;
mod context;
mod error;
mod ports;
mod types;

pub use app::SomaApplication;
pub use context::ExecutionContext;
pub use error::{ApplicationError, ApplicationErrorDetails};
pub use ports::{ApplicationPorts, CodeModePort, GatewayPort, OpenApiPort, PortError};
pub use types::CodeModeExecuteRequest;
pub use types::{
    CatalogSnapshot, DoctorReport, ElicitedName, ExecuteActionRequest, ExecuteActionResponse,
    GatewayExecuteRequest, GatewayPromptRoute, GatewayReloadRequest, GatewayResourceRoute,
    GatewayRouteScope, GatewayToolRoute, OpenApiExecuteRequest, OperationResponse,
    ReadResourceRequest, ResourceContent, ResourceTemplateSpec, ScaffoldIntentRequest,
};

pub use soma_provider_core::{ProviderPrompt, ProviderResource};
