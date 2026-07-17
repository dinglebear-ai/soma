//! Soma's own action catalog: the invariant action-spec table below and
//! request-parsing shared identically across REST, CLI, and MCP dispatch.
//!
//! See the module-placement note at the bottom of this file for why this
//! lives in `soma-domain` rather than `soma-application`.
//!
//! Note to editors: `xtask/src/patterns/actions.rs` and
//! `xtask/src/scripts_lane_d.rs` text-scan this file's *type and constant
//! names* (not just doc comments) starting from each name's first
//! occurrence in the file to locate the action-spec table below. Keep those
//! exact identifiers out of prose above the table itself, or the scan
//! anchors on the wrong occurrence and silently mis-parses. The rationale
//! comment at the bottom of this file exists specifically to stay clear of
//! that hazard.

use serde::Serialize;
use serde_json::{json, Value};

// ── Action error types ────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error(transparent)]
    Validation(#[from] ActionValidationError),
}

impl ActionError {
    pub fn as_validation(&self) -> Option<&ActionValidationError> {
        match self {
            Self::Validation(error) => Some(error),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActionValidationError {
    #[error("action is required")]
    MissingAction,
    #[error("`{field}` is required and must not be empty")]
    MissingField { field: String },
    #[error("`{field}` must be a string")]
    WrongType { field: String },
    #[error(
        "action={action} is not available over REST; use MCP or action=help for documentation"
    )]
    NotAvailableOverRest { action: String },
    #[error("unknown soma action: {action}; use action=help for documentation")]
    UnknownAction { action: String },
}

pub type ValidationError = ActionValidationError;

impl ActionValidationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingAction => "missing_action",
            Self::MissingField { .. } => "missing_field",
            Self::WrongType { .. } => "wrong_type",
            Self::NotAvailableOverRest { .. } => "not_available_over_rest",
            Self::UnknownAction { .. } => "unknown_action",
        }
    }

    pub fn field(&self) -> Option<&str> {
        match self {
            Self::MissingAction => Some("action"),
            Self::MissingField { field } | Self::WrongType { field } => Some(field.as_str()),
            Self::NotAvailableOverRest { .. } | Self::UnknownAction { .. } => Some("action"),
        }
    }

    pub fn bad_value(&self) -> Option<&str> {
        match self {
            Self::NotAvailableOverRest { action } | Self::UnknownAction { action } => {
                Some(action.as_str())
            }
            Self::MissingAction | Self::MissingField { .. } | Self::WrongType { .. } => None,
        }
    }

    pub fn remediation(&self) -> String {
        match self {
            Self::MissingAction => {
                format!(
                    "Set `action` to one of: {}. Use action=help for the full schema.",
                    action_names().join(", ")
                )
            }
            Self::MissingField { field } => {
                format!("Provide a non-empty `{field}` value, or use action=help for examples.")
            }
            Self::WrongType { field } => {
                format!("Pass `{field}` as a JSON string, or use action=help for examples.")
            }
            Self::NotAvailableOverRest { action } => {
                format!("Call action={action} through MCP, or call action=help over REST.")
            }
            Self::UnknownAction { .. } => {
                format!(
                    "Retry with one of the supported actions: {}. Use action=help for examples.",
                    action_names().join(", ")
                )
            }
        }
    }
}

// Scope constants and `scopes_satisfy` live in `crate::scopes` alongside
// `ADMIN_SCOPE` (formerly split across contracts' actions.rs and scopes.rs).
// Re-exported here so `soma_domain::actions::{READ_SCOPE, WRITE_SCOPE,
// DENY_SCOPE, scopes_satisfy}` keeps working for every call site that
// imports scope names through the action-metadata path.
pub use crate::scopes::{scopes_satisfy, DENY_SCOPE, READ_SCOPE, WRITE_SCOPE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTransport {
    Any,
    McpOnly,
}

impl ActionTransport {
    pub fn mcp(self) -> bool {
        matches!(self, Self::Any | Self::McpOnly)
    }

    pub fn cli(self) -> bool {
        matches!(self, Self::Any)
    }

    pub fn rest(self) -> bool {
        matches!(self, Self::Any)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionCost {
    Cheap,
    Moderate,
    Expensive,
    Write,
}

impl ActionCost {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cheap => "cheap",
            Self::Moderate => "moderate",
            Self::Expensive => "expensive",
            Self::Write => "write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub ty: &'static str,
    pub required: bool,
    pub description: &'static str,
    pub max_len: Option<usize>,
    pub enum_values: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliFlagSpec {
    pub name: &'static str,
    pub value_name: Option<&'static str>,
    pub required: bool,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliSpec {
    pub command: &'static str,
    pub usage: &'static str,
    pub flags: &'static [CliFlagSpec],
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required_scope: Option<&'static str>,
    pub transport: ActionTransport,
    pub rest_method: Option<&'static str>,
    pub rest_path: Option<&'static str>,
    pub destructive: bool,
    pub requires_admin: bool,
    pub cost: ActionCost,
    pub params: &'static [ParamSpec],
    pub returns: &'static str,
    pub cli: Option<CliSpec>,
}

const GREET_PARAMS: &[ParamSpec] = &[ParamSpec {
    name: "name",
    ty: "string",
    required: false,
    description: "Name to greet. Omit to greet the world.",
    max_len: Some(4096),
    enum_values: &[],
}];

const ECHO_PARAMS: &[ParamSpec] = &[ParamSpec {
    name: "message",
    ty: "string",
    required: true,
    description: "Message to echo back. Must not be empty.",
    max_len: Some(4096),
    enum_values: &[],
}];

const GREET_CLI_FLAGS: &[CliFlagSpec] = &[CliFlagSpec {
    name: "--name",
    value_name: Some("NAME"),
    required: false,
    description: "Name to greet. Omit to greet the world.",
}];

const ECHO_CLI_FLAGS: &[CliFlagSpec] = &[CliFlagSpec {
    name: "--message",
    value_name: Some("MSG"),
    required: true,
    description: "Message to echo back. Must not be empty.",
}];

pub const ACTION_SPECS: &[ActionSpec] = &[
    ActionSpec {
        name: "greet",
        description: "Return a greeting.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/greet"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: GREET_PARAMS,
        returns: "Greeting",
        cli: Some(CliSpec {
            command: "greet",
            usage: "soma greet [--name NAME]",
            flags: GREET_CLI_FLAGS,
            description: "Greet NAME, or the world when omitted.",
        }),
    },
    ActionSpec {
        name: "echo",
        description: "Echo a message back unchanged.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/echo"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: ECHO_PARAMS,
        returns: "EchoResult",
        cli: Some(CliSpec {
            command: "echo",
            usage: "soma echo --message MSG",
            flags: ECHO_CLI_FLAGS,
            description: "Echo MSG back unchanged.",
        }),
    },
    ActionSpec {
        name: "status",
        description: "Return server status and configuration info.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("GET"),
        rest_path: Some("/v1/status"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: &[],
        returns: "Status",
        cli: Some(CliSpec {
            command: "status",
            usage: "soma status",
            flags: &[],
            description: "Show service status.",
        }),
    },
    ActionSpec {
        name: "elicit_name",
        description: "Ask the MCP client to collect a name, then return a personalised greeting.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::McpOnly,
        rest_method: None,
        rest_path: None,
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: &[],
        returns: "Greeting",
        cli: None,
    },
    ActionSpec {
        name: "scaffold_intent",
        description: "Collect scaffold setup intent through MCP elicitation and return JSON for the scaffold-project skill.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::McpOnly,
        rest_method: None,
        rest_path: None,
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Moderate,
        params: &[],
        returns: "ScaffoldIntentReport",
        cli: None,
    },
    ActionSpec {
        name: "help",
        description: "Show the action reference.",
        required_scope: None,
        transport: ActionTransport::Any,
        rest_method: Some("GET"),
        rest_path: Some("/v1/help"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: &[],
        returns: "HelpPayload",
        cli: Some(CliSpec {
            command: "help",
            usage: "soma help",
            flags: &[],
            description: "Show JSON action reference.",
        }),
    },
];

pub fn action_names() -> Vec<&'static str> {
    ACTION_SPECS.iter().map(|spec| spec.name).collect()
}

pub fn is_known_action(action: &str) -> bool {
    ACTION_SPECS.iter().any(|spec| spec.name == action)
}

pub fn rest_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.transport.rest())
        .map(|spec| spec.name)
        .collect()
}

pub fn cli_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.cli.is_some())
        .map(|spec| spec.name)
        .collect()
}

pub fn cli_commands() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter_map(|spec| spec.cli.map(|cli| cli.command))
        .collect()
}

pub fn is_rest_action(action: &str) -> bool {
    action_spec(action)
        .map(|spec| spec.transport.rest())
        .unwrap_or(false)
}

pub fn mcp_only_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.transport == ActionTransport::McpOnly)
        .map(|spec| spec.name)
        .collect()
}

pub fn required_scope_for_action(action: &str) -> Option<&'static str> {
    action_spec(action)
        .map(|spec| spec.required_scope)
        .unwrap_or(Some(DENY_SCOPE))
}

pub fn action_spec(action: &str) -> Option<&'static ActionSpec> {
    ACTION_SPECS.iter().find(|spec| spec.name == action)
}

/// Confirmation gate for destructive actions, shared by every surface.
///
/// Returns `Err` with a structured validation error when `action` is marked
/// `destructive` in [`ACTION_SPECS`] and the caller did not pass
/// `"confirm": true` in `params`. Non-destructive actions (and unknown actions,
/// which fail later in scope/dispatch) always pass. Cheap and side-effect free —
/// call it on every dispatch so a future destructive action is gated by default
/// rather than by remembering to add a check.
pub fn require_confirmation_if_destructive(
    action: &str,
    params: &Value,
) -> Result<(), Box<crate::errors::ToolError>> {
    let Some(spec) = action_spec(action) else {
        return Ok(());
    };
    if !spec.destructive {
        return Ok(());
    }
    let confirmed = params
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if confirmed {
        return Ok(());
    }
    // Boxed because ToolError is a large struct and this is a hot, mostly-Ok path.
    Err(Box::new(
        crate::errors::ToolError::validation(
            "confirmation_required",
            format!("action '{action}' is destructive and requires \"confirm\": true"),
            "Re-send the request with \"confirm\": true to proceed.",
        )
        .with_field("confirm"),
    ))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SurfaceAvailability {
    pub mcp: bool,
    pub cli: bool,
    pub rest: bool,
    pub web_ui: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ParamDoc {
    pub name: String,
    pub ty: String,
    pub required: bool,
    pub description: String,
    pub max_len: Option<usize>,
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActionDoc {
    pub service: String,
    pub action: String,
    pub description: String,
    pub destructive: bool,
    pub requires_admin: bool,
    pub cost: String,
    pub required_scope: Option<String>,
    pub params: Vec<ParamDoc>,
    pub returns: String,
    pub surface_availability: SurfaceAvailability,
    pub auth_posture: String,
    pub mcp_only_exception: Option<String>,
    pub cli: Option<CliDoc>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CliFlagDoc {
    pub name: String,
    pub value_name: Option<String>,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CliDoc {
    pub command: String,
    pub usage: String,
    pub description: String,
    pub flags: Vec<CliFlagDoc>,
}

pub fn action_catalog() -> Vec<ActionDoc> {
    ACTION_SPECS
        .iter()
        .map(|spec| ActionDoc {
            service: "soma".to_owned(),
            action: spec.name.to_owned(),
            description: spec.description.to_owned(),
            destructive: spec.destructive,
            requires_admin: spec.requires_admin,
            cost: spec.cost.as_str().to_owned(),
            required_scope: spec.required_scope.map(ToOwned::to_owned),
            params: spec
                .params
                .iter()
                .map(|param| ParamDoc {
                    name: param.name.to_owned(),
                    ty: param.ty.to_owned(),
                    required: param.required,
                    description: param.description.to_owned(),
                    max_len: param.max_len,
                    enum_values: param
                        .enum_values
                        .iter()
                        .map(|value| (*value).to_owned())
                        .collect(),
                })
                .collect(),
            returns: spec.returns.to_owned(),
            surface_availability: SurfaceAvailability {
                mcp: spec.transport.mcp(),
                cli: spec.transport.cli(),
                rest: spec.transport.rest(),
                web_ui: false,
            },
            auth_posture: match spec.required_scope {
                Some(scope) => format!("requires `{scope}` on authenticated transports"),
                None => "public action; no action scope required".to_owned(),
            },
            mcp_only_exception: (spec.transport == ActionTransport::McpOnly)
                .then(|| "MCP-only because it requires client-rendered elicitation.".to_owned()),
            cli: spec.cli.map(|cli| CliDoc {
                command: cli.command.to_owned(),
                usage: cli.usage.to_owned(),
                description: cli.description.to_owned(),
                flags: cli
                    .flags
                    .iter()
                    .map(|flag| CliFlagDoc {
                        name: flag.name.to_owned(),
                        value_name: flag.value_name.map(ToOwned::to_owned),
                        required: flag.required,
                        description: flag.description.to_owned(),
                    })
                    .collect(),
            }),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SomaAction {
    Greet { name: Option<String> },
    Echo { message: String },
    Status,
    Help,
    ElicitName,
    ScaffoldIntent,
}

impl SomaAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Greet { .. } => "greet",
            Self::Echo { .. } => "echo",
            Self::Status => "status",
            Self::Help => "help",
            Self::ElicitName => "elicit_name",
            Self::ScaffoldIntent => "scaffold_intent",
        }
    }

    pub fn from_mcp_args(args: &Value) -> anyhow::Result<Self> {
        let action = match args.get("action") {
            None => return Err(action_error(ValidationError::MissingAction)),
            Some(Value::String(action)) => action.as_str(),
            Some(_) => {
                return Err(action_error(ValidationError::WrongType {
                    field: "action".into(),
                }));
            }
        };
        Self::from_params(action, args)
    }

    pub fn from_rest(action: &str, params: &Value) -> anyhow::Result<Self> {
        if action.is_empty() {
            return Err(action_error(ValidationError::MissingAction));
        }
        if action_spec(action)
            .map(|spec| spec.transport == ActionTransport::McpOnly)
            .unwrap_or(false)
        {
            return Err(action_error(ValidationError::NotAvailableOverRest {
                action: action.to_owned(),
            }));
        }
        Self::from_params(action, params)
    }

    fn from_params(action: &str, params: &Value) -> anyhow::Result<Self> {
        match action {
            "greet" => Ok(Self::Greet {
                name: optional_string_param(params, "name")?,
            }),
            "echo" => {
                let message = optional_string_param(params, "message")?
                    .filter(|m| !m.is_empty())
                    .ok_or_else(|| {
                        action_error(ValidationError::MissingField {
                            field: "message".into(),
                        })
                    })?;
                Ok(Self::Echo { message })
            }
            "status" => Ok(Self::Status),
            "help" => Ok(Self::Help),
            "elicit_name" => Ok(Self::ElicitName),
            "scaffold_intent" => Ok(Self::ScaffoldIntent),
            other => Err(action_error(ValidationError::UnknownAction {
                action: other.to_owned(),
            })),
        }
    }
}

pub fn rest_help() -> Value {
    json!({
        "actions": rest_action_names(),
        "mcp_only_actions": mcp_only_action_names(),
        "catalog": action_catalog(),
        "preferred_rest_style": "direct_routes",
        "usage": "Use direct REST routes such as POST /v1/echo or GET /v1/status. MCP keeps a single action-dispatched tool; REST does not expose an action envelope.",
        "examples": {
            "greet":  {"method": "POST", "path": "/v1/greet",  "body": {"name": "Alice"}},
            "echo":   {"method": "POST", "path": "/v1/echo",   "body": {"message": "Hello!"}},
            "status": {"method": "GET", "path": "/v1/status"},
        }
    })
}

fn optional_string_param(params: &Value, name: &str) -> Result<Option<String>, ValidationError> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| ValidationError::WrongType { field: name.into() }),
    }
}

fn action_error(error: ValidationError) -> anyhow::Error {
    error.into()
}

pub fn action_validation_error(error: &anyhow::Error) -> Option<&ActionValidationError> {
    if let Some(error) = error.downcast_ref::<ActionError>() {
        return error.as_validation();
    }
    error.downcast_ref::<ActionValidationError>()
}

pub fn is_validation_error(error: &anyhow::Error) -> bool {
    action_validation_error(error).is_some()
}

// ── Module-placement rationale ───────────────────────────────────────────────
//
// This module lives in soma-domain rather than soma-application (plan
// section 6.2 nominally assigns "product use-case request/results" to
// soma-application) because soma-service — a dependency of soma-application
// during the strangler migration — also builds its static-Rust provider
// catalog directly from this file's action-spec table and its per-entry
// struct type. Placing this module in soma-application would force
// soma-service to depend back on soma-application, creating a dependency
// cycle. Every consumer (application, service, api, cli, mcp, integrations,
// runtime, apps/soma) can already depend on soma-domain without cycles, so
// this is the lowest common ancestor that keeps everything cycle-free.

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
