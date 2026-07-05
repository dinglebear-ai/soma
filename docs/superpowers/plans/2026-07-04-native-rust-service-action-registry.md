# Native Rust Service Action Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native Rust service-owned action registry so a business action added in `rtemplate-service` becomes callable from MCP, REST, and CLI without editing `rtemplate-mcp`, `rtemplate-api`, or `rtemplate-cli`.

**Architecture:** Move static native action metadata and execution into `rtemplate-service`, then expose a cached `ActionRegistry` snapshot with O(1) lookup maps, derived help/catalog data, and provider-backed validation/auth helpers. MCP stays one tool with an `action` argument, REST uses direct `POST /v1/{action}` routes for registry actions, and CLI treats the first argument as either a built-in command or a registry action command. Future Wasm/TypeScript providers remain out of scope; do not introduce a provider trait until a second provider exists.

**Tech Stack:** Rust workspace crates, `serde_json::Value`, `std::sync::OnceLock`, `std::collections::HashMap`, Axum `POST /v1/{action}`, current rmcp single-tool server, existing `cargo xtask check-openapi`.

## Global Constraints

- Keep MCP as one action-dispatched tool; do not generate one MCP tool per action.
- Keep REST direct-route-only; `POST /v1/example` must not be present.
- Registry REST business actions are `POST`-only in this phase; keep existing explicit `GET /v1/status`, `GET /v1/help`, and `GET /v1/capabilities`.
- Future native business actions must require edits only in `crates/rtemplate-service`, plus generated docs/OpenAPI artifacts.
- Do not add `ActionProvider`, `StaticRustProvider`, `WasmProvider`, TypeScript, Node, Bun, AI SDK, `wasmtime`, or hot-reload code in this plan.
- Preserve destructive confirmation, scope checks, admin checks, structured errors, dispatch logging, metrics, and token-budget behavior.
- Generic `Json<Value>` input must be validated from registry metadata before dispatch: unknown fields denied, required fields checked, primitive types checked, duplicate CLI flags denied, and flag-looking values rejected.
- Help/catalog output must be cached and must support visibility filtering so public help does not leak future internal/admin action metadata.
- `reverse` is a test-only proof action behind `#[cfg(any(test, feature = "test-support"))]`; do not ship it as a normal public action.

---

## File Structure

Create:

- `crates/rtemplate-service/src/actions.rs` — single source of truth for native action specs, registry maps, validation, help/catalog generation, and execution.
- `crates/rtemplate-service/src/actions_tests.rs` — unit tests for registry lookup, validation, drift detection, cached help/catalog, and test-only proof action.

Modify:

- `crates/rtemplate-service/src/lib.rs` — re-export registry APIs and change `dispatch_action` to accept `(action: &str, params: &Value, surface: &str)`.
- `crates/rtemplate-contracts/src/actions.rs` — keep pure metadata/error types and provider-independent helper functions that accept `&[ActionSpec]`; remove or stop using contract-global `ACTION_SPECS` as a live registry.
- `crates/rtemplate-cli/src/lib.rs` and `crates/rtemplate-cli/src/cli_tests.rs` — parse dynamic registry actions as natural subcommands while keeping built-ins explicit.
- `crates/rtemplate-api/src/api.rs` — add generic `POST /v1/{action}` handler with provider-backed lookup, auth/scope/admin/confirmation, validation, dispatch, and response capping.
- `crates/rmcp-template/src/routes.rs` and `crates/rmcp-template/tests/api_routes.rs` — mount generic POST route and assert `/v1/example` is absent.
- `crates/rtemplate-mcp/src/rmcp_server.rs`, `crates/rtemplate-mcp/src/tools.rs`, `crates/rtemplate-mcp/src/schemas.rs`, and `crates/rmcp-template/tests/tool_dispatch.rs` — use the service registry for MCP schema, known-action checks, scope checks, destructive confirmation, and dispatch.
- `xtask/src/scripts_lane_d.rs` and `docs/generated/openapi.json` — generate OpenAPI from registry metadata without adding heavy service runtime coupling beyond metadata access.
- `README.md`, `docs/API.md`, `docs/ARCHITECTURE.md`, `docs/SERVICE_SURFACE_SUGGESTIONS.md`, `CHANGELOG.md` — document the add-action workflow and deferred runtime-provider direction.

---

### Task 1: Create Service-Owned Registry Snapshot

**Files:**
- Create: `crates/rtemplate-service/src/actions.rs`
- Create: `crates/rtemplate-service/src/actions_tests.rs`
- Modify: `crates/rtemplate-service/src/lib.rs`
- Modify: `crates/rtemplate-contracts/src/actions.rs`

**Interfaces:**
- Consumes: `ExampleService`, existing `ActionSpec`, `ActionTransport`, `ActionCost`, `ParamSpec`, `CliSpec`, `CliFlagSpec`, `ValidationError`.
- Produces:
  - `pub struct ActionRegistry`
  - `pub fn action_registry() -> &'static ActionRegistry`
  - `pub fn action_specs() -> &'static [ActionSpec]`
  - `pub async fn execute_native_action(service: &ExampleService, action: &str, params: &Value) -> anyhow::Result<Value>`
  - `pub async fn dispatch_action(service: &ExampleService, action: &str, params: &Value, surface: &str) -> anyhow::Result<Value>`

- [ ] **Step 1: Write failing registry tests**

Create `crates/rtemplate-service/src/actions_tests.rs`:

```rust
use serde_json::json;

use crate::actions::{action_registry, action_specs, execute_native_action, validate_params};
use crate::{ExampleClient, ExampleService};
use rtemplate_contracts::actions::{ActionTransport, READ_SCOPE};

#[test]
fn registry_has_single_source_action_metadata() {
    let names: Vec<_> = action_specs().iter().map(|spec| spec.name).collect();
    assert_eq!(
        names,
        vec!["greet", "echo", "status", "help", "elicit_name", "scaffold_intent"]
    );

    let echo = action_registry().action("echo").unwrap();
    assert_eq!(echo.required_scope, Some(READ_SCOPE));
    assert_eq!(echo.transport, ActionTransport::Any);
}

#[test]
fn registry_maps_cli_and_rest_without_linear_callers() {
    let registry = action_registry();
    assert_eq!(registry.cli_command("echo").unwrap().name, "echo");
    assert_eq!(registry.rest_post("echo").unwrap().name, "echo");
    assert!(registry.rest_post("status").is_none());
}

#[test]
fn param_validation_rejects_unknown_and_missing_fields() {
    let echo = action_registry().action("echo").unwrap();
    let missing = validate_params(echo, &json!({})).unwrap_err();
    assert!(missing.to_string().contains("message"));

    let unknown = validate_params(echo, &json!({"message": "hi", "extra": true})).unwrap_err();
    assert!(unknown.to_string().contains("unknown parameter"));
}

#[test]
fn param_validation_rejects_wrong_type_and_large_strings() {
    let echo = action_registry().action("echo").unwrap();
    let wrong_type = validate_params(echo, &json!({"message": 42})).unwrap_err();
    assert!(wrong_type.to_string().contains("must be a string"));

    let too_large = validate_params(echo, &json!({"message": "x".repeat(4097)})).unwrap_err();
    assert!(too_large.to_string().contains("too long"));
}

#[tokio::test]
async fn native_executor_dispatches_registered_action() {
    let cfg = rtemplate_contracts::config::ExampleConfig::default();
    let service = ExampleService::new(ExampleClient::new(&cfg).unwrap());
    let value = execute_native_action(&service, "echo", &json!({"message": "hello"}))
        .await
        .unwrap();
    assert_eq!(value["echo"], "hello");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p rtemplate-service actions_tests -- --nocapture
```

Expected: FAIL because `actions.rs`, `ActionRegistry`, `action_registry`, `execute_native_action`, and `validate_params` do not exist.

- [ ] **Step 3: Add metadata validation fields to contracts**

In `crates/rtemplate-contracts/src/actions.rs`, extend `ParamSpec` and `ActionSpec`:

```rust
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
pub enum CatalogVisibility {
    Public,
    Authenticated,
    Hidden,
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
    pub catalog_visibility: CatalogVisibility,
}
```

Add provider-independent helpers that accept explicit specs:

```rust
pub fn action_spec_from<'a>(specs: &'a [ActionSpec], action: &str) -> Option<&'a ActionSpec> {
    specs.iter().find(|spec| spec.name == action)
}

pub fn required_scope_for_action_from(specs: &[ActionSpec], action: &str) -> Option<&'static str> {
    action_spec_from(specs, action)
        .map(|spec| spec.required_scope)
        .unwrap_or(Some(DENY_SCOPE))
}

pub fn is_known_action_from(specs: &[ActionSpec], action: &str) -> bool {
    action_spec_from(specs, action).is_some()
}

pub fn require_confirmation_if_destructive_from(
    specs: &[ActionSpec],
    action: &str,
    params: &serde_json::Value,
) -> Result<(), Box<crate::errors::ToolError>> {
    let Some(spec) = action_spec_from(specs, action) else {
        return Ok(());
    };
    if !spec.destructive {
        return Ok(());
    }
    let confirmed = params
        .get("confirm")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if confirmed {
        return Ok(());
    }
    Err(Box::new(
        crate::errors::ToolError::validation(
            "confirmation_required",
            format!("action '{action}' is destructive and requires \"confirm\": true"),
            "Re-send the request with \"confirm\": true to proceed.",
        )
        .with_field("confirm"),
    ))
}
```

Keep existing contract wrappers temporarily only if they are still needed by untouched code, but add a comment above them:

```rust
// Transitional compatibility only. New code must pass rtemplate_service::action_specs()
// to the explicit *_from helpers so there is one live registry.
```

- [ ] **Step 4: Create service action registry**

Create `crates/rtemplate-service/src/actions.rs`:

```rust
use anyhow::Result;
use rtemplate_contracts::actions::{
    ActionCost, ActionSpec, ActionTransport, CatalogVisibility, CliFlagSpec, CliSpec, ParamSpec,
    READ_SCOPE,
};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ExampleService;

const MAX_STRING_PARAM_LEN: usize = 4096;

const GREET_PARAMS: &[ParamSpec] = &[ParamSpec {
    name: "name",
    ty: "string",
    required: false,
    description: "Name to greet. Omit to greet the world.",
    max_len: Some(MAX_STRING_PARAM_LEN),
    enum_values: &[],
}];

const ECHO_PARAMS: &[ParamSpec] = &[ParamSpec {
    name: "message",
    ty: "string",
    required: true,
    description: "Message to echo back. Must not be empty.",
    max_len: Some(MAX_STRING_PARAM_LEN),
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
            usage: "example greet [--name NAME]",
            flags: GREET_CLI_FLAGS,
            description: "Greet NAME, or the world when omitted.",
        }),
        catalog_visibility: CatalogVisibility::Public,
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
            usage: "example echo --message MSG",
            flags: ECHO_CLI_FLAGS,
            description: "Echo MSG back unchanged.",
        }),
        catalog_visibility: CatalogVisibility::Public,
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
            usage: "example status",
            flags: &[],
            description: "Show service status.",
        }),
        catalog_visibility: CatalogVisibility::Public,
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
            usage: "example help",
            flags: &[],
            description: "Show JSON action reference.",
        }),
        catalog_visibility: CatalogVisibility::Public,
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
        catalog_visibility: CatalogVisibility::Public,
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
        catalog_visibility: CatalogVisibility::Public,
    },
];

pub struct ActionRegistry {
    specs: &'static [ActionSpec],
    by_name: HashMap<&'static str, &'static ActionSpec>,
    by_cli_command: HashMap<&'static str, &'static ActionSpec>,
    rest_posts: HashMap<&'static str, &'static ActionSpec>,
    public_help: Value,
}

impl ActionRegistry {
    fn new(specs: &'static [ActionSpec]) -> Self {
        let mut by_name = HashMap::new();
        let mut by_cli_command = HashMap::new();
        let mut rest_posts = HashMap::new();
        for spec in specs {
            by_name.insert(spec.name, spec);
            if let Some(cli) = spec.cli {
                by_cli_command.insert(cli.command, spec);
            }
            if spec.transport.rest() && spec.rest_method == Some("POST") {
                rest_posts.insert(spec.name, spec);
            }
        }
        let public_help = build_help_payload(specs, false);
        Self {
            specs,
            by_name,
            by_cli_command,
            rest_posts,
            public_help,
        }
    }

    pub fn specs(&self) -> &'static [ActionSpec] {
        self.specs
    }

    pub fn action(&self, action: &str) -> Option<&'static ActionSpec> {
        self.by_name.get(action).copied()
    }

    pub fn cli_command(&self, command: &str) -> Option<&'static ActionSpec> {
        self.by_cli_command.get(command).copied()
    }

    pub fn rest_post(&self, action: &str) -> Option<&'static ActionSpec> {
        self.rest_posts.get(action).copied()
    }

    pub fn public_help(&self) -> Value {
        self.public_help.clone()
    }
}

static REGISTRY: OnceLock<ActionRegistry> = OnceLock::new();

pub fn action_registry() -> &'static ActionRegistry {
    REGISTRY.get_or_init(|| ActionRegistry::new(ACTION_SPECS))
}

pub fn action_specs() -> &'static [ActionSpec] {
    action_registry().specs()
}

pub fn validate_params(spec: &ActionSpec, params: &Value) -> Result<()> {
    let object = params.as_object().ok_or_else(|| {
        rtemplate_contracts::actions::action_error(
            rtemplate_contracts::actions::ValidationError::WrongType {
                field: "params".to_owned(),
            },
        )
    })?;
    validate_param_object(spec, object)
}

fn validate_param_object(spec: &ActionSpec, object: &Map<String, Value>) -> Result<()> {
    for key in object.keys() {
        if key == "confirm" {
            continue;
        }
        if !spec.params.iter().any(|param| param.name == key) {
            return Err(anyhow::anyhow!("unknown parameter `{key}` for action `{}`", spec.name));
        }
    }
    for param in spec.params {
        let value = object.get(param.name);
        if param.required && value.is_none() {
            return Err(rtemplate_contracts::actions::action_error(
                rtemplate_contracts::actions::ValidationError::MissingField {
                    field: param.name.to_owned(),
                },
            ));
        }
        let Some(value) = value else {
            continue;
        };
        match param.ty {
            "string" => {
                let Some(text) = value.as_str() else {
                    return Err(anyhow::anyhow!("parameter `{}` must be a string", param.name));
                };
                if let Some(max_len) = param.max_len {
                    if text.len() > max_len {
                        return Err(anyhow::anyhow!(
                            "parameter `{}` is too long: max {max_len} bytes",
                            param.name
                        ));
                    }
                }
                if !param.enum_values.is_empty() && !param.enum_values.contains(&text) {
                    return Err(anyhow::anyhow!(
                        "parameter `{}` must be one of: {}",
                        param.name,
                        param.enum_values.join(", ")
                    ));
                }
            }
            other => return Err(anyhow::anyhow!("unsupported parameter type `{other}`")),
        }
    }
    Ok(())
}

pub async fn execute_native_action(
    service: &ExampleService,
    action: &str,
    params: &Value,
) -> Result<Value> {
    let spec = action_registry()
        .action(action)
        .ok_or_else(|| rtemplate_contracts::actions::action_error(
            rtemplate_contracts::actions::ValidationError::UnknownAction {
                action: action.to_owned(),
            },
        ))?;
    validate_params(spec, params)?;
    match action {
        "greet" => service.greet(optional_string_param(params, "name")?.as_deref()).await,
        "echo" => service.echo(&required_string_param(params, "message")?).await,
        "status" => service.status().await,
        "help" => Ok(action_registry().public_help()),
        "elicit_name" => Err(anyhow::anyhow!("action=elicit_name requires an MCP peer")),
        "scaffold_intent" => Err(anyhow::anyhow!("action=scaffold_intent requires MCP elicitation")),
        other => Err(rtemplate_contracts::actions::action_error(
            rtemplate_contracts::actions::ValidationError::UnknownAction {
                action: other.to_owned(),
            },
        )),
    }
}

fn optional_string_param(params: &Value, name: &str) -> Result<Option<String>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| anyhow::anyhow!("parameter `{name}` must be a string")),
    }
}

fn required_string_param(params: &Value, name: &str) -> Result<String> {
    optional_string_param(params, name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| rtemplate_contracts::actions::action_error(
            rtemplate_contracts::actions::ValidationError::MissingField {
                field: name.to_owned(),
            },
        ))
}

fn build_help_payload(specs: &[ActionSpec], authenticated: bool) -> Value {
    let visible: Vec<&ActionSpec> = specs
        .iter()
        .filter(|spec| match spec.catalog_visibility {
            rtemplate_contracts::actions::CatalogVisibility::Public => true,
            rtemplate_contracts::actions::CatalogVisibility::Authenticated => authenticated,
            rtemplate_contracts::actions::CatalogVisibility::Hidden => false,
        })
        .collect();
    json!({
        "actions": visible.iter().filter(|spec| spec.transport.rest()).map(|spec| spec.name).collect::<Vec<_>>(),
        "mcp_only_actions": visible.iter().filter(|spec| spec.transport == ActionTransport::McpOnly).map(|spec| spec.name).collect::<Vec<_>>(),
        "preferred_rest_style": "direct_routes",
        "usage": "Use direct REST routes such as POST /v1/echo or GET /v1/status. MCP keeps a single action-dispatched tool; REST does not expose an action envelope.",
    })
}

#[cfg(any(test, feature = "test-support"))]
pub async fn execute_test_reverse(_service: &ExampleService, params: &Value) -> Result<Value> {
    validate_param_object(
        &ActionSpec {
            name: "reverse",
            description: "Reverse text for registry tests.",
            required_scope: Some(READ_SCOPE),
            transport: ActionTransport::Any,
            rest_method: Some("POST"),
            rest_path: Some("/v1/reverse"),
            destructive: false,
            requires_admin: false,
            cost: ActionCost::Cheap,
            params: &[ParamSpec {
                name: "text",
                ty: "string",
                required: true,
                description: "Text to reverse.",
                max_len: Some(MAX_STRING_PARAM_LEN),
                enum_values: &[],
            }],
            returns: "ReverseResult",
            cli: None,
            catalog_visibility: CatalogVisibility::Hidden,
        },
        params.as_object().ok_or_else(|| anyhow::anyhow!("params must be an object"))?,
    )?;
    let text = required_string_param(params, "text")?;
    let reversed: String = text.chars().rev().collect();
    Ok(json!({"text": text, "reversed": reversed}))
}
```

- [ ] **Step 5: Wire service dispatch through registry**

Modify `crates/rtemplate-service/src/lib.rs`:

```rust
pub mod actions;
pub mod app;
pub mod example;

use anyhow::Result;
use rtemplate_contracts::errors::{ServiceError, ToolError};
use serde_json::Value;

pub use actions::{action_registry, action_specs, execute_native_action, validate_params, ActionRegistry};
pub use app::{ElicitedNameOutcome, ExampleService, ScaffoldIntent, ScaffoldIntentValidationError};
pub use example::ExampleClient;

pub async fn dispatch_action(
    service: &ExampleService,
    action: &str,
    params: &Value,
    surface: &str,
) -> Result<Value> {
    let started = std::time::Instant::now();
    let result = execute_native_action(service, action, params).await;
    let elapsed_ms = started.elapsed().as_millis();
    let outcome = if result.is_ok() { "ok" } else { "error" };

    tracing::info!(
        surface,
        service = "example",
        action,
        outcome,
        elapsed_ms = elapsed_ms as u64,
        "action dispatched"
    );
    record_action_metric(surface, action, outcome, elapsed_ms as f64);

    result
}
```

Keep existing `record_action_metric`, `classify_service_error`, and `is_validation_error`. Update `classify_service_error` imports if `action_validation_error` remains in contracts.

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test -p rtemplate-service actions_tests -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rtemplate-service/src/actions.rs crates/rtemplate-service/src/actions_tests.rs crates/rtemplate-service/src/lib.rs crates/rtemplate-contracts/src/actions.rs
git commit -m "feat: add service action registry"
```

---

### Task 2: Make CLI Business Commands Registry-Driven

**Files:**
- Modify: `crates/rtemplate-cli/src/lib.rs`
- Modify: `crates/rtemplate-cli/src/cli_tests.rs`

**Interfaces:**
- Consumes: `rtemplate_service::action_registry()`, `rtemplate_service::dispatch_action(service, action, params, "cli")`.
- Produces:
  - `Command::Action { name: String, params: Value, yes: bool }`
  - natural dynamic commands such as `example echo --message hello`
  - built-ins remain explicit: `doctor`, `watch`, `setup`, `mcp`, `serve`, `--help`, `--version`

- [ ] **Step 1: Write failing dynamic CLI tests**

Add to `crates/rtemplate-cli/src/cli_tests.rs`:

```rust
use rtemplate_cli::{parse_args_from, Command};
use serde_json::json;

#[test]
fn dynamic_action_command_parses_registered_string_flags() {
    let command = parse_args_from(["echo", "--message", "hello"])
        .unwrap()
        .expect("command should parse");
    assert_eq!(
        command,
        Command::Action {
            name: "echo".to_owned(),
            params: json!({"message": "hello"}),
            yes: false,
        }
    );
}

#[test]
fn dynamic_action_rejects_duplicate_flags() {
    let error = parse_args_from(["echo", "--message", "one", "--message", "two"]).unwrap_err();
    assert!(error.to_string().contains("duplicate flag --message"));
}

#[test]
fn dynamic_action_rejects_flag_like_values() {
    let error = parse_args_from(["echo", "--message", "--bogus"]).unwrap_err();
    assert!(error.to_string().contains("looks like a flag"));
}

#[test]
fn dynamic_action_rejects_missing_required_flags() {
    let error = parse_args_from(["echo"]).unwrap_err();
    assert!(error.to_string().contains("missing required flag --message"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p rtemplate-cli dynamic_action -- --nocapture
```

Expected: FAIL because `Command::Action` does not exist.

- [ ] **Step 3: Replace business enum variants with generic action command**

In `crates/rtemplate-cli/src/lib.rs`, replace `Greet`, `Echo`, `Status`, and `Help` variants with:

```rust
Action {
    name: String,
    params: serde_json::Value,
    yes: bool,
},
```

Keep `Doctor`, `Watch`, and `Setup`.

- [ ] **Step 4: Add dynamic parser**

Add:

```rust
fn parse_dynamic_action_command(action: &str, rest: &[String]) -> Result<Option<Command>> {
    let Some(spec) = rtemplate_service::action_registry().cli_command(action) else {
        return Ok(None);
    };
    let mut params = serde_json::Map::new();
    let mut yes = false;
    let mut index = 0;
    while index < rest.len() {
        let flag = rest[index].as_str();
        if flag == "--yes" {
            yes = true;
            index += 1;
            continue;
        }
        let Some(cli) = spec.cli else {
            return Ok(None);
        };
        let Some(flag_spec) = cli.flags.iter().find(|candidate| candidate.name == flag) else {
            return Err(anyhow!("unknown flag {flag} for action {action}"));
        };
        let key = flag.trim_start_matches("--");
        if params.contains_key(key) {
            return Err(anyhow!("duplicate flag {flag} for action {action}"));
        }
        let Some(value) = rest.get(index + 1) else {
            return Err(anyhow!("{action} {flag} requires {}", flag_spec.value_name.unwrap_or("VALUE")));
        };
        if value.starts_with("--") {
            return Err(anyhow!("{action} {flag} value looks like a flag: {value}"));
        }
        params.insert(key.to_owned(), serde_json::Value::String(value.clone()));
        index += 2;
    }
    if let Some(cli) = spec.cli {
        for flag in cli.flags.iter().filter(|flag| flag.required) {
            let key = flag.name.trim_start_matches("--");
            if !params.contains_key(key) {
                return Err(anyhow!("missing required flag {}", flag.name));
            }
        }
    }
    let params = serde_json::Value::Object(params);
    rtemplate_service::validate_params(spec, &params)?;
    Ok(Some(Command::Action {
        name: spec.name.to_owned(),
        params,
        yes,
    }))
}
```

- [ ] **Step 5: Use the dynamic parser as the final parse fallback**

In `parse_args_from`, keep `doctor`, `watch`, and `setup` explicit. Replace business action arms with:

```rust
_ => parse_dynamic_action_command(subcommand, rest)?,
```

- [ ] **Step 6: Dispatch generic action commands**

Update `run`:

```rust
let result = match cmd {
    Command::Action { name, params, .. } => match dispatch_action(&service, name, params, "cli").await {
        Ok(value) => value,
        Err(error) => {
            let tool_error = classify_service_error(&error);
            eprintln!("{}", format_cli_tool_error(&tool_error));
            return Err(anyhow!(tool_error.message));
        }
    },
    Command::Doctor { .. } | Command::Watch { .. } | Command::Setup(_) => {
        unreachable!("dispatched directly in main.rs::run_cli")
    }
};
```

Update destructive confirmation:

```rust
fn confirm_command_if_destructive(cmd: &Command) -> Result<()> {
    let Command::Action { name, params, yes } = cmd else {
        return Ok(());
    };
    if *yes {
        return Ok(());
    }
    rtemplate_contracts::actions::require_confirmation_if_destructive_from(
        rtemplate_service::action_specs(),
        name,
        params,
    )
    .map_err(|error| anyhow!(error.message.clone()))
}
```

- [ ] **Step 7: Run CLI tests**

Run:

```bash
cargo test -p rtemplate-cli --tests
```

Expected: PASS after updating old tests to expect `Command::Action`.

- [ ] **Step 8: Commit**

```bash
git add crates/rtemplate-cli/src/lib.rs crates/rtemplate-cli/src/cli_tests.rs
git commit -m "feat: make cli actions registry driven"
```

---

### Task 3: Make REST Direct POST Routes Generic and Safe

**Files:**
- Modify: `crates/rtemplate-api/src/api.rs`
- Modify: `crates/rmcp-template/src/routes.rs`
- Modify: `crates/rmcp-template/tests/api_routes.rs`

**Interfaces:**
- Consumes: `rtemplate_service::action_registry()`, `rtemplate_service::validate_params`, `rtemplate_service::dispatch_action`.
- Produces: `POST /v1/{action}` for registry POST actions with provider-backed transport, auth/scope/admin, destructive confirmation, validation, and response capping.

- [ ] **Step 1: Write failing REST safety tests**

Add to `crates/rmcp-template/tests/api_routes.rs`:

```rust
#[tokio::test]
async fn generic_post_route_dispatches_registered_action() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/echo",
        None,
        Some(json!({"message": "hello"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["echo"], "hello");
}

#[tokio::test]
async fn generic_post_route_rejects_unknown_fields() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/echo",
        None,
        Some(json!({"message": "hello", "extra": true})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"].as_str().unwrap_or_default().contains("unknown parameter"));
}

#[tokio::test]
async fn removed_rest_envelope_is_not_found() {
    let app = server::router(loopback_state());
    let (status, _body) = request_json(
        app,
        Method::POST,
        "/v1/example",
        None,
        Some(json!({"action": "echo", "params": {"message": "hello"}})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p rmcp-template generic_post_route removed_rest_envelope -- --nocapture
```

Expected: FAIL until generic route and `/v1/example` removal are complete in this worktree.

- [ ] **Step 3: Add generic POST handler**

In `crates/rtemplate-api/src/api.rs`, add `Path` import and:

```rust
pub async fn v1_action_post(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(action): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let Json(params) = match body {
        Ok(body) => body,
        Err(error) => return rest_json_rejection_response(error),
    };

    let Some(spec) = rtemplate_service::action_registry().rest_post(&action) else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response();
    };

    run_rest_action_request(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        spec.name,
        params,
    )
    .await
}
```

- [ ] **Step 4: Preserve provider-backed REST auth, confirmation, and validation**

Change `run_rest_action_request` to:

```rust
async fn run_rest_action_request(
    state: AppState,
    auth: Option<&AuthContext>,
    action_name: &str,
    params: Value,
) -> axum::response::Response {
    let Some(spec) = rtemplate_service::action_registry().action(action_name) else {
        return rest_error_response(
            rtemplate_contracts::actions::action_error(
                rtemplate_contracts::actions::ValidationError::UnknownAction {
                    action: action_name.to_owned(),
                },
            ),
            action_name,
        );
    };
    if !spec.transport.rest() {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response();
    }
    if let Some(response) = enforce_rest_scope(&state, auth, action_name) {
        return response;
    }
    if spec.requires_admin {
        let Some(auth) = auth else {
            return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden: missing auth context"}))).into_response();
        };
        if !auth.scopes.iter().any(|scope| scope == "admin") {
            return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden: requires admin"}))).into_response();
        }
    }
    if let Err(error) = rtemplate_contracts::actions::require_confirmation_if_destructive_from(
        rtemplate_service::action_specs(),
        action_name,
        &params,
    ) {
        return (
            StatusCode::from_u16(error.http_status_code()).unwrap_or(StatusCode::BAD_REQUEST),
            Json(error.to_rest_payload()),
        )
            .into_response();
    }
    if let Err(error) = rtemplate_service::validate_params(spec, &params) {
        return rest_error_response(error, action_name);
    }
    match dispatch_action(&state.service, action_name, &params, "rest").await {
        Ok(value) => match cap_rest_response(value) {
            Ok(value) => Json(value).into_response(),
            Err(e) => {
                tracing::error!(error = %e, action = %action_name, "REST response serialization failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "internal server error"})),
                )
                    .into_response()
            }
        },
        Err(e) => rest_error_response(e, action_name),
    }
}
```

Keep `enforce_rest_scope`, but update it to use `required_scope_for_action_from(rtemplate_service::action_specs(), action)`.

- [ ] **Step 5: Update router**

In `crates/rmcp-template/src/routes.rs`, remove `api_dispatch`, `v1_greet`, and `v1_echo` imports. Add `v1_action_post`.

Use:

```rust
.route("/v1/capabilities", get(v1_capabilities))
.route("/v1/status", get(v1_service_status))
.route("/v1/help", get(v1_help))
.route("/v1/{action}", post(v1_action_post));
```

Do not mount `/v1/example`.

- [ ] **Step 6: Run REST route tests**

Run:

```bash
cargo test -p rmcp-template --test api_routes
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rtemplate-api/src/api.rs crates/rmcp-template/src/routes.rs crates/rmcp-template/tests/api_routes.rs
git commit -m "feat: route rest actions through registry"
```

---

### Task 4: Make MCP Registry-Backed End to End

**Files:**
- Modify: `crates/rtemplate-mcp/src/rmcp_server.rs`
- Modify: `crates/rtemplate-mcp/src/tools.rs`
- Modify: `crates/rtemplate-mcp/src/schemas.rs`
- Modify: `crates/rmcp-template/tests/tool_dispatch.rs`

**Interfaces:**
- Consumes: `rtemplate_service::action_specs()`, `rtemplate_service::action_registry()`, `rtemplate_service::dispatch_action`.
- Produces: MCP schema, known-action checks, scope checks, confirmation, and dispatch all use the service registry.

- [ ] **Step 1: Write failing full-path MCP tests**

Add to `crates/rmcp-template/tests/tool_dispatch.rs`:

```rust
#[tokio::test]
async fn full_mcp_call_tool_path_uses_service_registry() {
    let state = testing::loopback_state();
    let result = call_example_tool(&state, json!({"action": "echo", "message": "hello"}))
        .await
        .expect("tool should run");
    assert_eq!(result["echo"], "hello");
}
```

Use the existing full `call_tool` helper in this file if present. If no helper exists, add a helper that exercises `ServerHandler::call_tool` rather than `execute_tool_without_peer_for_test`.

- [ ] **Step 2: Run test to verify failure or baseline**

Run:

```bash
cargo test -p rmcp-template --test tool_dispatch full_mcp_call_tool_path_uses_service_registry -- --nocapture
```

Expected: FAIL until `rmcp_server.rs` no longer reads stale contract-global metadata.

- [ ] **Step 3: Update MCP server authorization gates**

In `crates/rtemplate-mcp/src/rmcp_server.rs`, replace imports and calls:

```rust
rtemplate_contracts::actions::is_known_action_from(rtemplate_service::action_specs(), action)
rtemplate_contracts::actions::required_scope_for_action_from(rtemplate_service::action_specs(), action)
rtemplate_contracts::actions::require_confirmation_if_destructive_from(rtemplate_service::action_specs(), action, args)
```

Remove use of contract-global `is_known_action`, `required_scope_for_action`, and `require_confirmation_if_destructive`.

- [ ] **Step 4: Update tool dispatch**

In `crates/rtemplate-mcp/src/tools.rs`, use:

```rust
let action = rtemplate_contracts::actions::action_name_from_mcp_args(&args)?;
match action {
    "elicit_name" => elicit_name(&state.service, peer).await,
    "scaffold_intent" => scaffold_intent(&state.service, peer).await,
    other => dispatch_action(&state.service, other, &args, "mcp").await,
}
```

For peerless tests, keep the MCP-only rejection path for `elicit_name` and `scaffold_intent`.

- [ ] **Step 5: Update MCP schema generation**

In `crates/rtemplate-mcp/src/schemas.rs`, replace `ACTION_SPECS` reads with:

```rust
rtemplate_service::action_specs()
```

Preserve existing `OnceLock` schema caching. Native static providers are immutable after process start.

- [ ] **Step 6: Run MCP tests**

Run:

```bash
cargo test -p rmcp-template --test tool_dispatch
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rtemplate-mcp/src/rmcp_server.rs crates/rtemplate-mcp/src/tools.rs crates/rtemplate-mcp/src/schemas.rs crates/rmcp-template/tests/tool_dispatch.rs
git commit -m "feat: make mcp action gates registry backed"
```

---

### Task 5: Generate OpenAPI and Docs From Service Registry

**Files:**
- Modify: `xtask/src/scripts_lane_d.rs`
- Modify: `docs/generated/openapi.json`
- Modify: `README.md`
- Modify: `docs/API.md`
- Modify: `docs/SERVICE_SURFACE_SUGGESTIONS.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `rtemplate_service::action_specs()`.
- Produces: OpenAPI and docs that reflect registry actions, direct REST routes, no `/v1/example`, and the native add-action workflow.

- [ ] **Step 1: Update OpenAPI validation**

In `xtask/src/scripts_lane_d.rs`, ensure validation contains:

```rust
if value.pointer("/paths/~1v1~1example").is_some() {
    failures.push("/v1/example must not be present; REST uses direct routes only".to_owned());
}
for action in rtemplate_service::action_specs()
    .iter()
    .filter(|spec| spec.transport.rest())
{
    let Some(path) = action.rest_path else {
        failures.push(format!("REST action {} is missing rest_path", action.name));
        continue;
    };
    let method = action.rest_method.unwrap_or("POST").to_ascii_lowercase();
    if value
        .pointer(&format!("/paths/{}/{}", escape_pointer(path), method))
        .is_none()
    {
        failures.push(format!("missing OpenAPI path for {} {path}", method.to_uppercase()));
    }
}
```

- [ ] **Step 2: Generate from registry metadata**

Replace local parsed action entry collection for REST route generation with:

```rust
let rest_actions: Vec<_> = rtemplate_service::action_specs()
    .iter()
    .filter(|spec| spec.transport.rest())
    .collect();
```

Generate request schemas from `spec.params`, including `max_len` as `maxLength`, `enum_values` as `enum`, required params, and `additionalProperties: false`.

- [ ] **Step 3: Regenerate OpenAPI**

Run:

```bash
cargo xtask check-openapi --write
cargo xtask check-openapi --check
```

Expected: PASS and no `/v1/example`.

- [ ] **Step 4: Update docs**

Add to `docs/SERVICE_SURFACE_SUGGESTIONS.md`:

```markdown
## Native Rust Action Add Workflow

To add a native Rust action:

1. Add the business method to `crates/rtemplate-service/src/app.rs` or a focused service module.
2. Add one action metadata entry and one executor match arm in `crates/rtemplate-service/src/actions.rs`.
3. Run `cargo test -p rtemplate-service -p rtemplate-cli -p rmcp-template --tests`.
4. Run `cargo xtask check-openapi --write`.

No edits should be required in `crates/rtemplate-api`, `crates/rtemplate-cli`, or `crates/rtemplate-mcp`.
```

Add to `CHANGELOG.md`:

```markdown
- Added a native Rust service action registry so MCP, REST, and CLI surfaces derive business actions from one service-owned registry.
```

- [ ] **Step 5: Commit**

```bash
git add xtask/src/scripts_lane_d.rs docs/generated/openapi.json README.md docs/API.md docs/SERVICE_SURFACE_SUGGESTIONS.md CHANGELOG.md
git commit -m "docs: describe service action registry"
```

---

### Task 6: Prove New Native Actions Require No Surface Edits

**Files:**
- Modify: `crates/rtemplate-service/src/actions.rs`
- Modify: `crates/rtemplate-service/src/actions_tests.rs`
- Modify: `crates/rmcp-template/tests/api_routes.rs`
- Modify: `crates/rmcp-template/tests/tool_dispatch.rs`
- Modify: `crates/rtemplate-cli/src/cli_tests.rs`

**Interfaces:**
- Consumes: registry from prior tasks.
- Produces: test-only `reverse` proof action behind `#[cfg(any(test, feature = "test-support"))]`; no production `reverse` action appears in default docs/OpenAPI.

- [ ] **Step 1: Add test-only proof action tests**

Add tests that use a test registry extension or existing `execute_test_reverse` helper:

```rust
#[tokio::test]
async fn test_only_reverse_proves_action_execution_without_surface_code() {
    let cfg = rtemplate_contracts::config::ExampleConfig::default();
    let service = ExampleService::new(ExampleClient::new(&cfg).unwrap());
    let value = crate::actions::execute_test_reverse(&service, &json!({"text": "stressed"}))
        .await
        .unwrap();
    assert_eq!(value["reversed"], "desserts");
}
```

Add route/CLI/MCP tests only if the test registry extension can be enabled without shipping `reverse` in normal metadata. If this requires intrusive runtime registry mutation, skip surface tests and instead add a compile-time assertion that adding a production action touches only service registry code.

- [ ] **Step 2: Ensure no production reverse route ships**

Run:

```bash
rg -n '"/v1/reverse"|reverse' docs/generated/openapi.json README.md docs/API.md
```

Expected: no matches outside test-only code.

- [ ] **Step 3: Commit**

```bash
git add crates/rtemplate-service/src/actions.rs crates/rtemplate-service/src/actions_tests.rs crates/rmcp-template/tests/api_routes.rs crates/rmcp-template/tests/tool_dispatch.rs crates/rtemplate-cli/src/cli_tests.rs
git commit -m "test: prove registry action extension path"
```

---

### Task 7: Final Verification

**Files:**
- Modify only if verification reveals drift.

**Interfaces:**
- Consumes: all prior tasks.
- Produces: verified registry-driven native action system.

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt --check
```

Expected: PASS. If it fails, run `cargo fmt`, then rerun `cargo fmt --check`.

- [ ] **Step 2: Run full tests**

Run:

```bash
cargo test --workspace --all-features
```

Expected: PASS.

- [ ] **Step 3: Run clippy**

Run:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: PASS.

- [ ] **Step 4: Run generated artifact checks**

Run:

```bash
cargo xtask check-openapi --check
git diff --check
```

Expected: PASS.

- [ ] **Step 5: Sweep for stale surface-specific action code**

Run:

```bash
rg -n "api_dispatch|ActionRequest|/v1/example|Command::Greet|Command::Echo|Command::Status|Command::Help|StaticRustProvider|ActionProvider|WasmProvider" crates docs README.md
```

Expected: no active code references. Historical `docs/sessions/*` references are acceptable only if the path is clearly a historical note. `docs/SERVICE_SURFACE_SUGGESTIONS.md` may mention runtime providers as deferred future work without defining code APIs.

- [ ] **Step 6: Final commit if verification changed files**

```bash
git status --short
git add .
git commit -m "chore: finalize service action registry"
```

Skip this commit if the worktree is already clean.

---

## Engineering Review Applied

- Removed premature `ActionProvider`, `StaticRustProvider`, and `STATIC_RUST_PROVIDER`.
- Added cached `ActionRegistry` maps for action, CLI command, and REST POST lookup.
- Made `rtemplate-service::action_specs()` the only live registry for new code.
- Required provider-backed scope, admin, destructive confirmation, and validation before generic REST dispatch.
- Required MCP `rmcp_server.rs` known-action, scope, and confirmation gates to use service registry metadata.
- Added metadata-driven input validation to replace typed REST DTO protections.
- Added duplicate CLI flag and flag-looking value rejection.
- Cached help/catalog payloads and added catalog visibility metadata.
- Kept REST registry actions POST-only for this phase.
- Changed `reverse` from shipped product action to test-only proof action.
- Deferred Wasm/TypeScript providers, hot reload, provider invalidation, rich schemas, and benchmarks.

## Self-Review

Spec coverage:

- Implement business in service crate and expose through all surfaces: Tasks 1 through 6.
- No API/CLI/MCP edits for future native business actions: Task 6 proof plus registry-driven Tasks 2 through 4.
- MCP remains one tool: Task 4.
- CLI supports natural subcommands without constant `--action`: Task 2.
- REST remains direct-route-only and removes `/v1/example`: Task 3.
- Build-time benefit from stable surfaces: achieved by eliminating recurring surface edits.
- Future `WasmProvider`/TypeScript providers are documented as deferred, not implemented.

Placeholder scan:

- No unresolved placeholder markers or undefined future-only steps are used.

Type consistency:

- The plan consistently uses `ActionRegistry`, `action_registry()`, `action_specs()`, `execute_native_action(...)`, `validate_params(...)`, and `dispatch_action(service, action, params, surface)`.
- CLI consistently uses `Command::Action { name, params, yes }`.
- REST consistently uses `POST /v1/{action}` for registry actions while preserving explicit GET routes for status/help/capabilities.
