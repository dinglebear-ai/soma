use std::{
    future::{ready, Future},
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::protocol::{
    ApplyPatchApprovalResponse, CommandExecutionApprovalDecision,
    CommandExecutionRequestApprovalResponse, CurrentTimeReadResponse, ExecCommandApprovalResponse,
    FileChangeApprovalDecision, FileChangeRequestApprovalResponse, GrantedPermissionProfile,
    McpServerElicitationAction, McpServerElicitationRequestResponse, PermissionGrantScope,
    PermissionsRequestApprovalResponse, RequestPermissionProfile, ReviewDecision, ServerRequest,
};
use crate::{PendingServerRequest, Result};

/// Boxed future returned by [`ApprovalHandler`].
///
/// The future is lifetime-bound to the handler and request so custom handlers
/// can borrow their own UI/channel state and inspect the typed request while
/// waiting asynchronously for a decision.
pub type ApprovalFuture<'a> = Pin<Box<dyn Future<Output = ServerRequestReply> + Send + 'a>>;

/// A typed reply for a server-to-client app-server request.
///
/// `Result` is any JSON value that matches the request's generated response
/// type. `Error` sends a JSON-RPC error reply to the app-server.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerRequestReply {
    Result(serde_json::Value),
    Error {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
}

impl ServerRequestReply {
    pub fn send(self, request: PendingServerRequest) -> Result<()> {
        match self {
            Self::Result(result) => request.respond(result),
            Self::Error {
                code,
                message,
                data,
            } => request.respond_error(code, message, data),
        }
    }
}

/// Policy hook for server-to-client app-server requests.
///
/// Implementations return a future so human-in-the-loop UI, channels, or other
/// async policy engines can decide without blocking a Tokio worker while the
/// session drains app-server events.
pub trait ApprovalHandler: Send + Sync {
    fn handle<'a>(&'a self, request: &'a ServerRequest) -> ApprovalFuture<'a>;
}

/// Approval handler that rejects every server request with a JSON-RPC error.
///
/// This is the safest first-mile default for examples and smoke tests: Codex
/// can keep running, but no command, file-change, permission, elicitation, or
/// dynamic-tool request is silently accepted.
#[derive(Clone, Debug)]
pub struct DenyAllApprovalHandler {
    code: i64,
    message: String,
}

impl Default for DenyAllApprovalHandler {
    fn default() -> Self {
        Self {
            code: -32000,
            message: "codex-app-server-client: no approval handler accepted this request"
                .to_owned(),
        }
    }
}

impl DenyAllApprovalHandler {
    /// Creates a deny-all handler with a custom error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Self::default()
        }
    }
}

/// Approval handler that approves Codex approval requests.
///
/// This answers command, file-change, legacy command, legacy patch, and
/// permission-profile approval requests with the schema's positive response.
/// Requests that require app-specific data, such as dynamic tool calls, user
/// input, auth-token refreshes, and MCP elicitations, are rejected with a clear
/// JSON-RPC error instead of guessing.
#[derive(Clone, Debug, Default)]
pub struct AllowAllApprovalHandler;

impl ApprovalHandler for AllowAllApprovalHandler {
    fn handle<'a>(&'a self, request: &'a ServerRequest) -> ApprovalFuture<'a> {
        Box::pin(ready(match request {
            ServerRequest::ItemCommandExecutionRequestApproval { .. } => {
                serialized_result(CommandExecutionRequestApprovalResponse {
                    decision: CommandExecutionApprovalDecision::Accept,
                })
            }
            ServerRequest::ItemFileChangeRequestApproval { .. } => {
                serialized_result(FileChangeRequestApprovalResponse {
                    decision: FileChangeApprovalDecision::Accept,
                })
            }
            ServerRequest::ItemPermissionsRequestApproval { params, .. } => {
                serialized_result(PermissionsRequestApprovalResponse {
                    permissions: grant_requested_permissions(&params.permissions),
                    scope: PermissionGrantScope::Turn,
                    strict_auto_review: None,
                })
            }
            ServerRequest::ApplyPatchApproval { .. } => {
                serialized_result(ApplyPatchApprovalResponse {
                    decision: ReviewDecision::Approved,
                })
            }
            ServerRequest::ExecCommandApproval { .. } => {
                serialized_result(ExecCommandApprovalResponse {
                    decision: ReviewDecision::Approved,
                })
            }
            ServerRequest::CurrentTimeRead { .. } => current_time_reply(),
            ServerRequest::McpServerElicitationRequest { .. } => {
                serialized_result(McpServerElicitationRequestResponse {
                    action: McpServerElicitationAction::Decline,
                    content: None,
                    meta: None,
                })
            }
            _ => {
                unsupported_request_error(request, "allow-all approval policy has no canned reply")
            }
        }))
    }
}

/// Approval handler for read-only integrations.
///
/// This answers `currentTime/read` and declines command/file-change approval
/// prompts without interrupting the turn. Permission escalations and other
/// app-specific server requests receive JSON-RPC errors so callers can handle
/// them explicitly if needed.
#[derive(Clone, Debug, Default)]
pub struct ReadOnlyApprovalHandler;

impl ApprovalHandler for ReadOnlyApprovalHandler {
    fn handle<'a>(&'a self, request: &'a ServerRequest) -> ApprovalFuture<'a> {
        Box::pin(ready(match request {
            ServerRequest::ItemCommandExecutionRequestApproval { .. } => {
                serialized_result(CommandExecutionRequestApprovalResponse {
                    decision: CommandExecutionApprovalDecision::Decline,
                })
            }
            ServerRequest::ItemFileChangeRequestApproval { .. } => {
                serialized_result(FileChangeRequestApprovalResponse {
                    decision: FileChangeApprovalDecision::Decline,
                })
            }
            ServerRequest::ApplyPatchApproval { .. } => {
                serialized_result(ApplyPatchApprovalResponse {
                    decision: ReviewDecision::Denied,
                })
            }
            ServerRequest::ExecCommandApproval { .. } => {
                serialized_result(ExecCommandApprovalResponse {
                    decision: ReviewDecision::Denied,
                })
            }
            ServerRequest::CurrentTimeRead { .. } => current_time_reply(),
            ServerRequest::McpServerElicitationRequest { .. } => {
                serialized_result(McpServerElicitationRequestResponse {
                    action: McpServerElicitationAction::Decline,
                    content: None,
                    meta: None,
                })
            }
            _ => unsupported_request_error(
                request,
                "read-only approval policy declined this request",
            ),
        }))
    }
}

impl ApprovalHandler for DenyAllApprovalHandler {
    fn handle<'a>(&'a self, _request: &'a ServerRequest) -> ApprovalFuture<'a> {
        Box::pin(ready(ServerRequestReply::Error {
            code: self.code,
            message: self.message.clone(),
            data: None,
        }))
    }
}

/// Approval handler backed by a closure.
///
/// Use this when the crate's canned policies are too broad or too narrow. The
/// closure receives the typed [`ServerRequest`] and returns the exact reply to
/// send back to the app-server.
pub struct FnApprovalHandler<F>(F);

impl<F> FnApprovalHandler<F>
where
    F: Fn(&ServerRequest) -> ServerRequestReply + Send + Sync,
{
    /// Wraps a closure as an [`ApprovalHandler`].
    pub fn new(handler: F) -> Self {
        Self(handler)
    }
}

impl<F> ApprovalHandler for FnApprovalHandler<F>
where
    F: Fn(&ServerRequest) -> ServerRequestReply + Send + Sync,
{
    fn handle<'a>(&'a self, request: &'a ServerRequest) -> ApprovalFuture<'a> {
        Box::pin(ready((self.0)(request)))
    }
}

/// Approval handler backed by an async closure.
///
/// Use this for UI, channel, or service-backed approval policies that need to
/// await a decision while a turn is being drained.
pub struct AsyncFnApprovalHandler<F>(F);

impl<F> AsyncFnApprovalHandler<F>
where
    F: for<'a> Fn(&'a ServerRequest) -> ApprovalFuture<'a> + Send + Sync,
{
    /// Wraps an async closure as an [`ApprovalHandler`].
    pub fn new(handler: F) -> Self {
        Self(handler)
    }
}

impl<F> ApprovalHandler for AsyncFnApprovalHandler<F>
where
    F: for<'a> Fn(&'a ServerRequest) -> ApprovalFuture<'a> + Send + Sync,
{
    fn handle<'a>(&'a self, request: &'a ServerRequest) -> ApprovalFuture<'a> {
        (self.0)(request)
    }
}

fn grant_requested_permissions(requested: &RequestPermissionProfile) -> GrantedPermissionProfile {
    GrantedPermissionProfile {
        file_system: requested.file_system.clone(),
        network: requested.network.clone(),
    }
}

fn current_time_reply() -> ServerRequestReply {
    let current_time_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    serialized_result(CurrentTimeReadResponse { current_time_at })
}

fn serialized_result<T>(response: T) -> ServerRequestReply
where
    T: serde::Serialize,
{
    ServerRequestReply::Result(
        serde_json::to_value(response).expect("generated app-server response types serialize"),
    )
}

fn unsupported_request_error(request: &ServerRequest, message: &'static str) -> ServerRequestReply {
    ServerRequestReply::Error {
        code: -32000,
        message: format!(
            "codex-app-server-client: {message} for {}",
            request.method_name()
        ),
        data: Some(serde_json::json!({
            "method": request.method_name(),
            "expectedResponseType": request.expected_response_type_name(),
        })),
    }
}
