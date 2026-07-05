//! MCP tool dispatch — thin shims only.
//!
//! **Rule**: no business logic here. Parse args → call service → return Value.
//! All logic belongs in `app.rs` (or `example.rs` for transport concerns).
//!
//! The `peer` parameter is threaded through so that elicitation actions can
//! ask the MCP client for user input mid-call. For non-elicitation actions
//! it is unused.

use rmcp::{
    service::{ElicitationError, Peer},
    RoleServer,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(test)]
use std::sync::OnceLock;

#[cfg(test)]
use rtemplate_contracts::actions::{ActionTransport, ACTION_SPECS};
use rtemplate_runtime::server::AppState;
use rtemplate_service::app::{ElicitedNameOutcome, ExampleService, ScaffoldIntent};
use rtemplate_service::{
    ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRequestLimits, ProviderSurface,
};

/// Dispatch an incoming MCP tool call to the appropriate handler.
///
/// `name`   — tool name (matches schema, currently only "example")
/// `args`   — parsed JSON arguments from the MCP client
/// `peer`   — connection to the MCP client; used for elicitation
pub(super) async fn execute_tool(
    state: &AppState,
    name: &str,
    args: Value,
    peer: &Peer<RoleServer>,
    principal: ProviderPrincipal,
    auth_mode: ProviderAuthMode,
) -> anyhow::Result<Value> {
    match name {
        "example" => dispatch_example(state, args, peer, principal, auth_mode).await,
        _ => Err(anyhow::anyhow!("unknown tool: {name}")),
    }
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub async fn execute_tool_without_peer_for_test(
    state: &AppState,
    name: &str,
    args: Value,
) -> anyhow::Result<Value> {
    match name {
        "example" => {
            dispatch_example_without_peer(
                state,
                args,
                ProviderPrincipal::loopback_dev(),
                ProviderAuthMode::LoopbackDev,
            )
            .await
        }
        _ => Err(anyhow::anyhow!("unknown tool: {name}")),
    }
}

async fn dispatch_example(
    state: &AppState,
    args: Value,
    peer: &Peer<RoleServer>,
    principal: ProviderPrincipal,
    auth_mode: ProviderAuthMode,
) -> anyhow::Result<Value> {
    let action = action_name(&args)?.to_owned();

    match action.as_str() {
        "elicit_name" => elicit_name(&state.service, peer).await,
        "scaffold_intent" => scaffold_intent(&state.service, peer).await,
        other => dispatch_non_elicitation_action(state, other, args, principal, auth_mode).await,
    }
}

#[cfg(any(test, feature = "test-support"))]
async fn dispatch_example_without_peer(
    state: &AppState,
    args: Value,
    principal: ProviderPrincipal,
    auth_mode: ProviderAuthMode,
) -> anyhow::Result<Value> {
    let action = action_name(&args)?.to_owned();
    match action.as_str() {
        "elicit_name" | "scaffold_intent" => {
            Err(anyhow::anyhow!("action={} requires an MCP peer", action))
        }
        other => dispatch_non_elicitation_action(state, other, args, principal, auth_mode).await,
    }
}

async fn dispatch_non_elicitation_action(
    state: &AppState,
    action: &str,
    args: Value,
    principal: ProviderPrincipal,
    auth_mode: ProviderAuthMode,
) -> anyhow::Result<Value> {
    state
        .provider_registry
        .refresh_file_providers()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let params = strip_action_arg(args);
    let call = ProviderCall {
        provider: String::new(),
        action: action.to_owned(),
        params,
        principal,
        auth_mode,
        surface: ProviderSurface::Mcp,
        destructive_confirmed: false,
        limits: ProviderRequestLimits::default(),
        snapshot_id: String::new(),
    };
    Ok(state.provider_registry.dispatch(call).await?.value)
}

fn action_name(args: &Value) -> anyhow::Result<&str> {
    args.get("action")
        .and_then(Value::as_str)
        .filter(|action| !action.is_empty())
        .ok_or_else(|| anyhow::anyhow!("action is required"))
}

fn strip_action_arg(mut args: Value) -> Value {
    if let Value::Object(map) = &mut args {
        map.remove("action");
        map.remove("_response_offset");
        map.remove("_response_page_bytes");
        map.remove("_response_cursor");
    }
    args
}

// ── elicitation ───────────────────────────────────────────────────────────────

/// Input schema for the elicit_name elicitation request.
///
/// `ElicitationSafe` requires this to be a struct (object schema) — not a primitive.
/// The MCP client renders this as a form for the user to fill in.
///
/// Add `#[schemars(description = "...")]` on fields to provide field-level hints
/// in the client's UI.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct NameInput {
    /// Your first name (or whatever you'd like to be called)
    name: String,
}

// Mark as safe for elicitation — proves this type generates an "object" JSON schema.
rmcp::elicit_safe!(NameInput);

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ScaffoldIntentInput {
    /// Human-readable project name, e.g. "Unraid MCP" or "Lab Gateway"
    display_name: String,
    /// Cargo package name, e.g. "unraid-mcp"
    crate_name: String,
    /// Binary/tool name, e.g. "unraid"
    binary_name: String,
    /// Server category: "upstream-client" or "application-platform"
    server_category: String,
    /// Environment variable prefix, e.g. "UNRAID" or "LAB"
    env_prefix: String,
    /// Upstream auth kind: "none", "api-key", "bearer", "oauth", "both", or "other"
    auth_kind: String,
    /// Default bind host, e.g. "127.0.0.1" or "0.0.0.0"
    host: String,
    /// Default HTTP port, e.g. 3100
    port: u16,
    /// MCP transport mode: "stdio", "http", or "dual"
    mcp_transport: String,
    /// MCP primitives to scaffold, comma-separated: "tools, resources, prompts, elicitation"
    mcp_primitives: String,
    /// Deployment scaffolding: "none", "systemd", or "docker"
    deployment: String,
    /// Plugin surfaces to scaffold, comma-separated: "claude, codex, gemini". Leave blank for none.
    plugins: String,
    /// Whether to scaffold MCP registry publishing through server.json
    publish_mcp: bool,
    /// Documentation URLs to crawl via Axon, comma-separated. Leave blank if none.
    crawl_urls: String,
    /// Repository URLs to crawl via Axon, comma-separated. Leave blank if none.
    crawl_repos: String,
    /// Search topics to crawl via Axon, comma-separated. Leave blank if none.
    crawl_search_topics: String,
}

rmcp::elicit_safe!(ScaffoldIntentInput);

/// Ask the MCP client to collect scaffold requirements, then return JSON for a skill handoff.
///
/// This function intentionally does not mutate files. The returned JSON is consumed by
/// the `scaffold-project` skill, which drafts an approval-first plan for the user.
///
/// # How MCP elicitation works
///
/// Elicitation is an MCP protocol feature (spec 2025-06-18) where the *server*
/// requests input from the *user* (via the MCP client) mid-call:
///
/// 1. Server sends `elicitation/create` with a message and a JSON Schema
/// 2. Client displays a form to the user
/// 3. User fills in the form and submits (accept), refuses (decline), or closes (cancel)
/// 4. Client returns the user's choice + their data back to the server
/// 5. Server processes the response and continues the tool call
///
/// `peer.elicit::<T>()` handles the schema generation and response parsing automatically.
///
/// # Client compatibility
///
/// Only clients that declared the `elicitation` capability during the MCP initialisation
/// handshake will respond. If the client doesn't support it, this returns a graceful
/// fallback message rather than an error.
async fn scaffold_intent(
    service: &ExampleService,
    peer: &Peer<RoleServer>,
) -> anyhow::Result<Value> {
    match peer
        .elicit::<ScaffoldIntentInput>(
            "Tell me what kind of project you are scaffolding. I will return JSON only; the scaffold-project skill will turn it into an approval-first plan.",
        )
        .await
    {
        Ok(Some(input)) => service.scaffold_intent(input.into()),
        Ok(None) => Ok(json!({
            "kind": "rmcp_template_scaffold_intent",
            "schema_version": 1,
            "status": "no_input",
            "message": "No scaffold intent was provided.",
        })),
        Err(ElicitationError::UserDeclined) => Ok(json!({
            "kind": "rmcp_template_scaffold_intent",
            "schema_version": 1,
            "status": "declined",
            "message": "User declined to provide scaffold intent.",
        })),
        Err(ElicitationError::UserCancelled) => Ok(json!({
            "kind": "rmcp_template_scaffold_intent",
            "schema_version": 1,
            "status": "cancelled",
            "message": "Scaffold intent elicitation was cancelled.",
        })),
        Err(ElicitationError::CapabilityNotSupported) => Ok(json!({
            "kind": "rmcp_template_scaffold_intent",
            "schema_version": 1,
            "status": "elicitation_not_supported",
            "message": "This MCP client does not support elicitation.",
            "fallback": {
                "recommended_skill": "scaffold-project",
                "instructions": "Ask the user for the scaffold fields manually, then create the same JSON shape documented by the scaffold-project skill. Do not mutate files until the user approves the plan."
            }
        })),
        Err(e) => {
            tracing::error!(error = %e, "scaffold intent elicitation failed unexpectedly");
            Err(anyhow::anyhow!("scaffold intent elicitation failed unexpectedly: {e}"))
        }
    }
}

impl From<ScaffoldIntentInput> for ScaffoldIntent {
    fn from(input: ScaffoldIntentInput) -> Self {
        Self {
            display_name: input.display_name,
            crate_name: input.crate_name,
            binary_name: input.binary_name,
            server_category: input.server_category,
            env_prefix: input.env_prefix,
            auth_kind: input.auth_kind,
            host: input.host,
            port: input.port,
            mcp_transport: input.mcp_transport,
            mcp_primitives: input.mcp_primitives,
            deployment: input.deployment,
            plugins: input.plugins,
            publish_mcp: input.publish_mcp,
            crawl_urls: input.crawl_urls,
            crawl_repos: input.crawl_repos,
            crawl_search_topics: input.crawl_search_topics,
        }
    }
}

async fn elicit_name(service: &ExampleService, peer: &Peer<RoleServer>) -> anyhow::Result<Value> {
    match peer
        .elicit::<NameInput>("What is your name? I'll use it to give you a personalised greeting.")
        .await
    {
        Ok(Some(input)) => {
            Ok(service.elicited_name_greeting(ElicitedNameOutcome::Accepted(&input.name)))
        }
        Ok(None) => Ok(service.elicited_name_greeting(ElicitedNameOutcome::NoInput)),
        Err(ElicitationError::UserDeclined) => {
            Ok(service.elicited_name_greeting(ElicitedNameOutcome::Declined))
        }
        Err(ElicitationError::UserCancelled) => {
            Ok(service.elicited_name_greeting(ElicitedNameOutcome::Cancelled))
        }
        Err(ElicitationError::CapabilityNotSupported) => {
            tracing::warn!("elicitation requested but client does not support it");
            Ok(service.elicited_name_greeting(ElicitedNameOutcome::Unsupported))
        }
        Err(e) => {
            tracing::error!(error = %e, "elicitation failed unexpectedly");
            Err(anyhow::anyhow!("elicitation failed unexpectedly: {e}"))
        }
    }
}

// ── arg helpers ───────────────────────────────────────────────────────────────

// ── help text ─────────────────────────────────────────────────────────────────

#[cfg(test)]
static HELP_TEXT: OnceLock<String> = OnceLock::new();

#[cfg(test)]
fn help_text() -> &'static str {
    HELP_TEXT.get_or_init(build_help_text).as_str()
}

#[cfg(test)]
fn build_help_text() -> String {
    let mut text = String::from(
        "# example MCP Tool\n\nA template demonstrating the action-based dispatch pattern for MCP servers.\nSet the `action` argument to select an operation.\n\n## Actions\n",
    );

    for spec in ACTION_SPECS {
        text.push_str("\n### ");
        text.push_str(spec.name);
        text.push('\n');
        text.push_str(spec.description);
        text.push('\n');
        text.push_str("- Returns: `");
        text.push_str(spec.returns);
        text.push_str("`\n");
        match spec.required_scope {
            Some(scope) => {
                text.push_str("- Scope: `");
                text.push_str(scope);
                text.push_str("`\n");
            }
            None => text.push_str("- Scope: public\n"),
        }
        if spec.destructive {
            text.push_str("- Destructive: yes\n");
        }
        if spec.requires_admin {
            text.push_str("- Requires admin: yes\n");
        }
        if spec.transport == ActionTransport::McpOnly {
            text.push_str("- Surface: MCP only\n");
        } else {
            text.push_str("- Surfaces: MCP, CLI, REST\n");
        }
        if spec.params.is_empty() {
            text.push_str("- Params: none\n");
        } else {
            text.push_str("- Params:\n");
            for param in spec.params {
                let required = if param.required {
                    "required"
                } else {
                    "optional"
                };
                text.push_str("  - `");
                text.push_str(param.name);
                text.push_str("` (");
                text.push_str(param.ty);
                text.push_str(", ");
                text.push_str(required);
                text.push_str("): ");
                text.push_str(param.description);
                text.push('\n');
            }
        }
    }

    text.push_str(
        "\n## Adding a new action

1. Add the action metadata to `ACTION_SPECS` in `actions.rs`.
2. Add any new parameters to the action's `params` metadata.
3. Add a method to `ExampleClient` in `example.rs` (transport).
4. Add a method to `ExampleService` in `app.rs` (business logic).
5. Add a match arm in `dispatch_example()` in `mcp/tools.rs`.
6. Add a test covering parser, schema, and dispatch behavior.
",
    );
    text
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
