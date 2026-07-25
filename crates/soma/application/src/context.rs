use soma_domain::{AuthorizationMode, Confirmation, Principal, RequestId, Surface, TraceContext};

/// Per-request execution context threaded through the application layer,
/// carrying caller identity, authorization mode, surface, and limits.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Authenticated caller, or `None` when no principal is attached.
    pub principal: Option<Principal>,
    /// How authorization is enforced for this request.
    pub authorization_mode: AuthorizationMode,
    /// Surface (MCP, REST, CLI, ...) the request arrived on.
    pub surface: Surface,
    /// Inbound distributed-trace context, when propagated.
    pub trace: Option<TraceContext>,
    /// Whether the caller supplied confirmation for destructive actions.
    pub destructive_confirmation: Confirmation,
    /// Optional cap on the response payload size.
    pub response_limit: Option<usize>,
    /// Unique identifier for this request.
    pub request_id: RequestId,
}

impl ExecutionContext {
    /// Builds a loopback-dev context with no principal and no limits, for
    /// trusted local calls where auth is bypassed.
    pub fn loopback(surface: Surface, request_id: RequestId) -> Self {
        Self {
            principal: None,
            authorization_mode: AuthorizationMode::LoopbackDev,
            surface,
            trace: None,
            destructive_confirmation: Confirmation::Missing,
            response_limit: None,
            request_id,
        }
    }
}
