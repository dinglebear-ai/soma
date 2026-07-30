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
use serde_json::{Value, json};

// ── Action error types ────────────────────────────────────────────────────────

/// Top-level error for action parsing and validation.
#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    /// A request failed input validation (missing/wrong-typed field, unknown action, etc.).
    #[error(transparent)]
    Validation(#[from] ActionValidationError),
}

impl ActionError {
    /// Returns the inner [`ActionValidationError`] when this is a validation failure.
    pub fn as_validation(&self) -> Option<&ActionValidationError> {
        match self {
            Self::Validation(error) => Some(error),
        }
    }
}

/// Structured validation failures produced while parsing an action request.
#[derive(Debug, thiserror::Error)]
pub enum ActionValidationError {
    /// No `action` was supplied.
    #[error("action is required")]
    MissingAction,
    /// A required field was absent or empty.
    #[error("`{field}` is required and must not be empty")]
    MissingField {
        /// Name of the missing field.
        field: String,
    },
    /// A field was present but had the wrong JSON type (expected a string).
    #[error("`{field}` must be a string")]
    WrongType {
        /// Name of the wrongly-typed field.
        field: String,
    },
    /// The action exists but is MCP-only and cannot be called over REST.
    #[error("action={action} is not available over REST; use MCP or action=help for documentation")]
    NotAvailableOverRest {
        /// The requested action name.
        action: String,
    },
    /// The requested action name is not recognised.
    #[error("unknown soma action: {action}; use action=help for documentation")]
    UnknownAction {
        /// The unrecognised action name.
        action: String,
    },
}

/// Convenience alias for [`ActionValidationError`].
pub type ValidationError = ActionValidationError;

impl ActionValidationError {
    /// Returns a stable machine-readable error code for this variant.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingAction => "missing_action",
            Self::MissingField { .. } => "missing_field",
            Self::WrongType { .. } => "wrong_type",
            Self::NotAvailableOverRest { .. } => "not_available_over_rest",
            Self::UnknownAction { .. } => "unknown_action",
        }
    }

    /// Returns the name of the offending request field, if any.
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::MissingAction => Some("action"),
            Self::MissingField { field } | Self::WrongType { field } => Some(field.as_str()),
            Self::NotAvailableOverRest { .. } | Self::UnknownAction { .. } => Some("action"),
        }
    }

    /// Returns the offending action name for action-related variants, if any.
    pub fn bad_value(&self) -> Option<&str> {
        match self {
            Self::NotAvailableOverRest { action } | Self::UnknownAction { action } => {
                Some(action.as_str())
            }
            Self::MissingAction | Self::MissingField { .. } | Self::WrongType { .. } => None,
        }
    }

    /// Returns human-readable guidance for how to fix the request.
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
pub use crate::scopes::{DENY_SCOPE, READ_SCOPE, WRITE_SCOPE, scopes_satisfy};

/// Which transports an action is reachable over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTransport {
    /// Available over MCP, CLI, and REST.
    Any,
    /// Available over MCP only (e.g. actions requiring client-side elicitation).
    McpOnly,
}

impl ActionTransport {
    /// Whether the action is reachable over MCP.
    pub fn mcp(self) -> bool {
        matches!(self, Self::Any | Self::McpOnly)
    }

    /// Whether the action is reachable over the CLI.
    pub fn cli(self) -> bool {
        matches!(self, Self::Any)
    }

    /// Whether the action is reachable over REST.
    pub fn rest(self) -> bool {
        matches!(self, Self::Any)
    }
}

/// Relative cost / side-effect hint advertised for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionCost {
    /// Fast, cheap, and side-effect free.
    Cheap,
    /// Moderately expensive to compute.
    Moderate,
    /// Expensive (heavy compute or slow I/O).
    Expensive,
    /// Performs a write / mutating operation.
    Write,
}

impl ActionCost {
    /// Returns the lowercase string label for this cost tier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cheap => "cheap",
            Self::Moderate => "moderate",
            Self::Expensive => "expensive",
            Self::Write => "write",
        }
    }
}

/// Static specification of a single action parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSpec {
    /// Parameter name.
    pub name: &'static str,
    /// JSON type of the parameter (e.g. `"string"`).
    pub ty: &'static str,
    /// Whether the parameter is required.
    pub required: bool,
    /// Human-readable description of the parameter.
    pub description: &'static str,
    /// Maximum allowed length, when the parameter is length-bounded.
    pub max_len: Option<usize>,
    /// Permitted values when the parameter is an enum; empty otherwise.
    pub enum_values: &'static [&'static str],
}

/// Static specification of a single CLI flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliFlagSpec {
    /// Flag name (e.g. `--name`).
    pub name: &'static str,
    /// Metavariable for the flag's value, if it takes one.
    pub value_name: Option<&'static str>,
    /// Whether the flag is required.
    pub required: bool,
    /// Human-readable description of the flag.
    pub description: &'static str,
}

/// Static specification of an action's CLI surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliSpec {
    /// Subcommand name.
    pub command: &'static str,
    /// Usage string shown in help.
    pub usage: &'static str,
    /// Flags accepted by the subcommand.
    pub flags: &'static [CliFlagSpec],
    /// Human-readable description of the subcommand.
    pub description: &'static str,
}

/// Canonical, invariant metadata for a single Soma action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpec {
    /// Action name (the `action` dispatch key).
    pub name: &'static str,
    /// Human-readable description of what the action does.
    pub description: &'static str,
    /// Scope required to invoke the action, or `None` if public.
    pub required_scope: Option<&'static str>,
    /// Transports the action is reachable over.
    pub transport: ActionTransport,
    /// REST method for the action's direct route, if any.
    pub rest_method: Option<&'static str>,
    /// REST path for the action's direct route, if any.
    pub rest_path: Option<&'static str>,
    /// Whether the action is destructive and requires confirmation.
    pub destructive: bool,
    /// Whether the action requires admin privileges.
    pub requires_admin: bool,
    /// Advertised cost / side-effect tier.
    pub cost: ActionCost,
    /// Parameter specifications.
    pub params: &'static [ParamSpec],
    /// Name of the returned result type.
    pub returns: &'static str,
    /// CLI surface for the action, if it has one.
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

const PYTHON_ENVIRONMENT_PRUNE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "stale_before_unix_seconds",
        ty: "integer",
        required: true,
        description: "Only select non-ready cache entries last modified at or before this Unix timestamp.",
        max_len: None,
        enum_values: &[],
    },
    ParamSpec {
        name: "max_entries",
        ty: "integer",
        required: false,
        description: "Maximum cache entries to inspect in this bounded operation (default 100, maximum 1000).",
        max_len: None,
        enum_values: &[],
    },
];

const PYTHON_ENVIRONMENT_PRUNE_APPLY_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "stale_before_unix_seconds",
        ty: "integer",
        required: true,
        description: "Only remove non-ready cache entries last modified at or before this Unix timestamp.",
        max_len: None,
        enum_values: &[],
    },
    ParamSpec {
        name: "max_entries",
        ty: "integer",
        required: false,
        description: "Maximum cache entries to remove in this bounded operation (default 100, maximum 1000).",
        max_len: None,
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to apply the destructive prune plan.",
        max_len: None,
        enum_values: &[],
    },
];

const PYTHON_ENVIRONMENT_PROVIDER_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "provider_path",
        ty: "string",
        required: true,
        description: "Managed Python provider path, relative to the configured provider directory or absolute within it.",
        max_len: Some(4096),
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to mutate the provider environment lifecycle.",
        max_len: None,
        enum_values: &[],
    },
];

const PYTHON_WORKER_PROVIDER_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "provider",
        ty: "string",
        required: true,
        description: "Loaded persistent Python provider name.",
        max_len: Some(256),
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to interrupt or reset worker state.",
        max_len: None,
        enum_values: &[],
    },
];

const PYTHON_GENERATION_ROLLBACK_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "generation_id",
        ty: "integer",
        required: true,
        description: "Retained generation identifier to reactivate.",
        max_len: None,
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to replace the active generation.",
        max_len: None,
        enum_values: &[],
    },
];

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

/// The canonical table of every Soma action and its invariant metadata.
///
/// This is the single source of truth for action names, scopes, and transport
/// availability across REST, CLI, and MCP dispatch.
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
        name: "python_environment_status",
        description: "Inspect immutable Python environment cache state without executing provider code.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("GET"),
        rest_path: Some("/v1/python/environments"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: &[],
        returns: "PythonEnvironmentStatus",
        cli: Some(CliSpec {
            command: "python_environment_status",
            usage: "soma python_environment_status",
            flags: &[],
            description: "Inspect immutable Python environment cache state.",
        }),
    },
    ActionSpec {
        name: "python_environment_prune_plan",
        description: "Plan a bounded prune of stale non-ready Python environment cache entries.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/python/environments/prune-plan"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Moderate,
        params: PYTHON_ENVIRONMENT_PRUNE_PARAMS,
        returns: "PythonEnvironmentPrunePlan",
        cli: Some(CliSpec {
            command: "python_environment_prune_plan",
            usage: "soma python_environment_prune_plan --json '{\"stale_before_unix_seconds\": 0}'",
            flags: &[],
            description: "Preview a bounded cache prune without mutation.",
        }),
    },
    ActionSpec {
        name: "python_environment_prune",
        description: "Apply a bounded prune of stale non-ready Python environment cache entries.",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/python/environments/prune"),
        destructive: true,
        requires_admin: false,
        cost: ActionCost::Write,
        params: PYTHON_ENVIRONMENT_PRUNE_APPLY_PARAMS,
        returns: "PythonEnvironmentPruneReport",
        cli: Some(CliSpec {
            command: "python_environment_prune",
            usage: "soma python_environment_prune --json '{\"stale_before_unix_seconds\": 0, \"confirm\": true}'",
            flags: &[],
            description: "Apply a confirmed bounded cache prune.",
        }),
    },
    ActionSpec {
        name: "python_environment_repair",
        description: "Repair the immutable environment for one managed Python provider.",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/python/environments/repair"),
        destructive: true,
        requires_admin: false,
        cost: ActionCost::Write,
        params: PYTHON_ENVIRONMENT_PROVIDER_PARAMS,
        returns: "PythonEnvironmentRepairReport",
        cli: Some(CliSpec {
            command: "python_environment_repair",
            usage: "soma python_environment_repair --json '{\"provider_path\": \"example.py\", \"confirm\": true}'",
            flags: &[],
            description: "Repair one managed provider environment.",
        }),
    },
    ActionSpec {
        name: "python_environment_update",
        description: "Resolve, prepare, validate, and atomically activate an immutable update for one managed Python provider.",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/python/environments/update"),
        destructive: true,
        requires_admin: false,
        cost: ActionCost::Write,
        params: PYTHON_ENVIRONMENT_PROVIDER_PARAMS,
        returns: "PythonEnvironmentUpdateReport",
        cli: Some(CliSpec {
            command: "python_environment_update",
            usage: "soma python_environment_update --json '{\"provider_path\": \"example.py\", \"confirm\": true}'",
            flags: &[],
            description: "Prepare and atomically activate one provider update.",
        }),
    },
    ActionSpec {
        name: "python_worker_status",
        description: "Inspect persistent Python worker health, quarantine, restart counts, and bounded redacted logs.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("GET"),
        rest_path: Some("/v1/python/workers"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: &[],
        returns: "PythonWorkerStatus",
        cli: Some(CliSpec {
            command: "python_worker_status",
            usage: "soma python_worker_status",
            flags: &[],
            description: "Inspect persistent Python worker state and logs.",
        }),
    },
    ActionSpec {
        name: "python_worker_cancel",
        description: "Cancel one active persistent Python invocation by terminating its process tree.",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/python/workers/cancel"),
        destructive: true,
        requires_admin: false,
        cost: ActionCost::Write,
        params: PYTHON_WORKER_PROVIDER_PARAMS,
        returns: "PythonWorkerCancellation",
        cli: Some(CliSpec {
            command: "python_worker_cancel",
            usage: "soma python_worker_cancel --json '{\"provider\": \"example\", \"confirm\": true}'",
            flags: &[],
            description: "Cancel an active persistent Python invocation.",
        }),
    },
    ActionSpec {
        name: "python_worker_reset",
        description: "Clear one persistent Python worker crash-loop quarantine.",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/python/workers/reset"),
        destructive: true,
        requires_admin: false,
        cost: ActionCost::Write,
        params: PYTHON_WORKER_PROVIDER_PARAMS,
        returns: "PythonWorkerReset",
        cli: Some(CliSpec {
            command: "python_worker_reset",
            usage: "soma python_worker_reset --json '{\"provider\": \"example\", \"confirm\": true}'",
            flags: &[],
            description: "Clear a persistent worker quarantine.",
        }),
    },
    ActionSpec {
        name: "python_generation_status",
        description: "Inspect the active Python provider generation and bounded rollback history.",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("GET"),
        rest_path: Some("/v1/python/generations"),
        destructive: false,
        requires_admin: false,
        cost: ActionCost::Cheap,
        params: &[],
        returns: "PythonGenerationStatus",
        cli: Some(CliSpec {
            command: "python_generation_status",
            usage: "soma python_generation_status",
            flags: &[],
            description: "Inspect active and retained Python provider generations.",
        }),
    },
    ActionSpec {
        name: "python_generation_rollback",
        description: "Atomically reactivate a retained Python provider generation.",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        rest_method: Some("POST"),
        rest_path: Some("/v1/python/generations/rollback"),
        destructive: true,
        requires_admin: false,
        cost: ActionCost::Write,
        params: PYTHON_GENERATION_ROLLBACK_PARAMS,
        returns: "PythonGenerationRollback",
        cli: Some(CliSpec {
            command: "python_generation_rollback",
            usage: "soma python_generation_rollback --json '{\"generation_id\": 1, \"confirm\": true}'",
            flags: &[],
            description: "Atomically reactivate a retained generation.",
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

/// Returns the names of every known action.
pub fn action_names() -> Vec<&'static str> {
    ACTION_SPECS.iter().map(|spec| spec.name).collect()
}

/// Returns whether `action` is a known action name.
pub fn is_known_action(action: &str) -> bool {
    ACTION_SPECS.iter().any(|spec| spec.name == action)
}

/// Returns the names of actions reachable over REST.
pub fn rest_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.transport.rest())
        .map(|spec| spec.name)
        .collect()
}

/// Returns the names of actions that expose a CLI surface.
pub fn cli_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.cli.is_some())
        .map(|spec| spec.name)
        .collect()
}

/// Returns the CLI subcommand names for every action that has one.
pub fn cli_commands() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter_map(|spec| spec.cli.map(|cli| cli.command))
        .collect()
}

/// Returns whether `action` is reachable over REST.
pub fn is_rest_action(action: &str) -> bool {
    action_spec(action)
        .map(|spec| spec.transport.rest())
        .unwrap_or(false)
}

/// Returns the names of actions that are MCP-only.
pub fn mcp_only_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.transport == ActionTransport::McpOnly)
        .map(|spec| spec.name)
        .collect()
}

/// Returns the scope required to invoke `action`.
///
/// Unknown actions return [`DENY_SCOPE`] so they fail closed.
pub fn required_scope_for_action(action: &str) -> Option<&'static str> {
    action_spec(action)
        .map(|spec| spec.required_scope)
        .unwrap_or(Some(DENY_SCOPE))
}

/// Looks up the [`ActionSpec`] for `action`, if it exists.
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

/// Serializable flags describing which surfaces expose an action.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SurfaceAvailability {
    /// Reachable over MCP.
    pub mcp: bool,
    /// Reachable over the CLI.
    pub cli: bool,
    /// Reachable over REST.
    pub rest: bool,
    /// Reachable from the web UI.
    pub web_ui: bool,
}

/// Serializable documentation for a single action parameter.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ParamDoc {
    /// Parameter name.
    pub name: String,
    /// JSON type of the parameter.
    pub ty: String,
    /// Whether the parameter is required.
    pub required: bool,
    /// Human-readable description of the parameter.
    pub description: String,
    /// Maximum allowed length, when length-bounded.
    pub max_len: Option<usize>,
    /// Permitted values when the parameter is an enum.
    pub enum_values: Vec<String>,
}

/// Serializable, catalog-facing documentation for a single action.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActionDoc {
    /// Owning service name (always `"soma"`).
    pub service: String,
    /// Action name.
    pub action: String,
    /// Human-readable description of the action.
    pub description: String,
    /// Whether the action is destructive.
    pub destructive: bool,
    /// Whether the action requires admin privileges.
    pub requires_admin: bool,
    /// Advertised cost tier label.
    pub cost: String,
    /// Scope required to invoke the action, or `None` if public.
    pub required_scope: Option<String>,
    /// Parameter documentation.
    pub params: Vec<ParamDoc>,
    /// Name of the returned result type.
    pub returns: String,
    /// Which surfaces expose the action.
    pub surface_availability: SurfaceAvailability,
    /// Human-readable summary of the action's auth requirements.
    pub auth_posture: String,
    /// Explanation of why the action is MCP-only, when applicable.
    pub mcp_only_exception: Option<String>,
    /// CLI documentation for the action, if it has a CLI surface.
    pub cli: Option<CliDoc>,
}

/// Serializable documentation for a single CLI flag.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CliFlagDoc {
    /// Flag name.
    pub name: String,
    /// Metavariable for the flag's value, if it takes one.
    pub value_name: Option<String>,
    /// Whether the flag is required.
    pub required: bool,
    /// Human-readable description of the flag.
    pub description: String,
}

/// Serializable documentation for an action's CLI surface.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CliDoc {
    /// Subcommand name.
    pub command: String,
    /// Usage string shown in help.
    pub usage: String,
    /// Human-readable description of the subcommand.
    pub description: String,
    /// Flags accepted by the subcommand.
    pub flags: Vec<CliFlagDoc>,
}

/// Builds the serializable action catalog from [`ACTION_SPECS`].
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

/// A parsed, validated action request ready for dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SomaAction {
    /// Return a greeting, optionally addressed to `name`.
    Greet {
        /// Name to greet; greets the world when absent.
        name: Option<String>,
    },
    /// Echo `message` back unchanged.
    Echo {
        /// Message to echo back.
        message: String,
    },
    /// Return server status and configuration info.
    Status,
    /// Inspect immutable Python environment cache state.
    PythonEnvironmentStatus,
    /// Plan a bounded prune of stale non-ready Python environment entries.
    PythonEnvironmentPrunePlan {
        /// Oldest modification timestamp selected by the plan.
        stale_before_unix_seconds: u64,
        /// Maximum number of entries included in the plan.
        max_entries: usize,
    },
    /// Apply a bounded prune of stale non-ready Python environment entries.
    PythonEnvironmentPrune {
        /// Oldest modification timestamp selected by the plan.
        stale_before_unix_seconds: u64,
        /// Maximum number of entries removed.
        max_entries: usize,
    },
    /// Repair one managed Python provider environment.
    PythonEnvironmentRepair {
        /// Provider path relative to the configured provider root, or an absolute managed path.
        provider_path: String,
    },
    /// Prepare and atomically activate one managed Python provider update.
    PythonEnvironmentUpdate {
        /// Provider path relative to the configured provider root, or an absolute managed path.
        provider_path: String,
    },
    /// Inspect persistent Python worker state and bounded redacted logs.
    PythonWorkerStatus,
    /// Cancel one active persistent Python invocation.
    PythonWorkerCancel {
        /// Loaded persistent Python provider name.
        provider: String,
    },
    /// Clear one persistent Python worker crash-loop quarantine.
    PythonWorkerReset {
        /// Loaded persistent Python provider name.
        provider: String,
    },
    /// Inspect active and retained Python provider generations.
    PythonGenerationStatus,
    /// Atomically reactivate one retained generation.
    PythonGenerationRollback {
        /// Retained generation identifier.
        generation_id: u64,
    },
    /// Show the action reference.
    Help,
    /// Ask the MCP client to collect a name, then greet it (MCP-only).
    ElicitName,
    /// Collect scaffold setup intent via MCP elicitation (MCP-only).
    ScaffoldIntent,
}

impl SomaAction {
    /// Returns the canonical action name for this variant.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Greet { .. } => "greet",
            Self::Echo { .. } => "echo",
            Self::Status => "status",
            Self::PythonEnvironmentStatus => "python_environment_status",
            Self::PythonEnvironmentPrunePlan { .. } => "python_environment_prune_plan",
            Self::PythonEnvironmentPrune { .. } => "python_environment_prune",
            Self::PythonEnvironmentRepair { .. } => "python_environment_repair",
            Self::PythonEnvironmentUpdate { .. } => "python_environment_update",
            Self::PythonWorkerStatus => "python_worker_status",
            Self::PythonWorkerCancel { .. } => "python_worker_cancel",
            Self::PythonWorkerReset { .. } => "python_worker_reset",
            Self::PythonGenerationStatus => "python_generation_status",
            Self::PythonGenerationRollback { .. } => "python_generation_rollback",
            Self::Help => "help",
            Self::ElicitName => "elicit_name",
            Self::ScaffoldIntent => "scaffold_intent",
        }
    }

    /// Parses an action from an MCP tool-call argument object.
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

    /// Parses an action from a REST request, rejecting MCP-only actions.
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
            "python_environment_status" => Ok(Self::PythonEnvironmentStatus),
            "python_environment_prune_plan" => Ok(Self::PythonEnvironmentPrunePlan {
                stale_before_unix_seconds: required_u64_param(params, "stale_before_unix_seconds")?,
                max_entries: bounded_entries(params)?,
            }),
            "python_environment_prune" => Ok(Self::PythonEnvironmentPrune {
                stale_before_unix_seconds: required_u64_param(params, "stale_before_unix_seconds")?,
                max_entries: bounded_entries(params)?,
            }),
            "python_environment_repair" => Ok(Self::PythonEnvironmentRepair {
                provider_path: required_string_param(params, "provider_path")?,
            }),
            "python_environment_update" => Ok(Self::PythonEnvironmentUpdate {
                provider_path: required_string_param(params, "provider_path")?,
            }),
            "python_worker_status" => Ok(Self::PythonWorkerStatus),
            "python_worker_cancel" => Ok(Self::PythonWorkerCancel {
                provider: required_string_param(params, "provider")?,
            }),
            "python_worker_reset" => Ok(Self::PythonWorkerReset {
                provider: required_string_param(params, "provider")?,
            }),
            "python_generation_status" => Ok(Self::PythonGenerationStatus),
            "python_generation_rollback" => Ok(Self::PythonGenerationRollback {
                generation_id: required_u64_param(params, "generation_id")?,
            }),
            "help" => Ok(Self::Help),
            "elicit_name" => Ok(Self::ElicitName),
            "scaffold_intent" => Ok(Self::ScaffoldIntent),
            other => Err(action_error(ValidationError::UnknownAction {
                action: other.to_owned(),
            })),
        }
    }
}

/// Builds the JSON help payload describing the REST surface and examples.
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

fn required_string_param(params: &Value, name: &str) -> Result<String, ValidationError> {
    optional_string_param(params, name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ValidationError::MissingField { field: name.into() })
}

fn required_u64_param(params: &Value, name: &str) -> Result<u64, ValidationError> {
    params
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| ValidationError::WrongType { field: name.into() })
}

fn bounded_entries(params: &Value) -> Result<usize, ValidationError> {
    let entries = params.get("max_entries").map_or(Ok(100_u64), |value| {
        value.as_u64().ok_or_else(|| ValidationError::WrongType {
            field: "max_entries".into(),
        })
    })?;
    if !(1..=1000).contains(&entries) {
        return Err(ValidationError::WrongType {
            field: "max_entries".into(),
        });
    }
    Ok(entries as usize)
}

fn action_error(error: ValidationError) -> anyhow::Error {
    error.into()
}

/// Extracts an [`ActionValidationError`] from a boxed [`anyhow::Error`], if present.
pub fn action_validation_error(error: &anyhow::Error) -> Option<&ActionValidationError> {
    if let Some(error) = error.downcast_ref::<ActionError>() {
        return error.as_validation();
    }
    error.downcast_ref::<ActionValidationError>()
}

/// Returns whether `error` is (or wraps) an action validation error.
pub fn is_validation_error(error: &anyhow::Error) -> bool {
    action_validation_error(error).is_some()
}

// ── Module-placement rationale ───────────────────────────────────────────────
//
// This module lives in soma-domain rather than soma-application (plan
// section 6.2 nominally assigns "product use-case request/results" to
// soma-application) even though PR 19 folded the legacy soma-service crate's
// business layer (`SomaService`, the static-Rust provider catalog) directly
// into soma-application, which would no longer create a cross-crate cycle if
// this table moved there too. It stays in soma-domain because ACTION_SPECS is
// an invariant product contract (action names, scopes, transport
// availability), not application orchestration logic, and every consumer
// (application, api, cli, mcp, integrations, runtime, apps/soma, xtask) can
// depend on soma-domain without cycles regardless of what else changes inside
// soma-application.

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
