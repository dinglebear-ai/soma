use anyhow::Result;
use rtemplate_contracts::actions::{
    ActionCost, ActionSpec, ActionTransport, CatalogVisibility, CliFlagSpec, CliSpec, ParamSpec,
    READ_SCOPE,
};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ExampleService;

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

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
        if key == "confirm" || key == "action" {
            continue;
        }
        if !spec.params.iter().any(|param| param.name == key) {
            return Err(anyhow::anyhow!(
                "unknown parameter `{key}` for action `{}`",
                spec.name
            ));
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
                    return Err(anyhow::anyhow!(
                        "parameter `{}` must be a string",
                        param.name
                    ));
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
    let spec = action_registry().action(action).ok_or_else(|| {
        rtemplate_contracts::actions::action_error(
            rtemplate_contracts::actions::ValidationError::UnknownAction {
                action: action.to_owned(),
            },
        )
    })?;
    validate_params(spec, params)?;
    match action {
        "greet" => service.greet(optional_string_param(params, "name")?.as_deref()).await,
        "echo" => service.echo(&required_string_param(params, "message")?).await,
        "status" => service.status().await,
        "help" => Ok(action_registry().public_help()),
        "elicit_name" => Err(anyhow::anyhow!("action=elicit_name requires an MCP peer")),
        "scaffold_intent" => Err(anyhow::anyhow!(
            "action=scaffold_intent requires MCP elicitation"
        )),
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
        .ok_or_else(|| {
            rtemplate_contracts::actions::action_error(
                rtemplate_contracts::actions::ValidationError::MissingField {
                    field: name.to_owned(),
                },
            )
        })
}

fn build_help_payload(specs: &[ActionSpec], authenticated: bool) -> Value {
    let visible: Vec<&ActionSpec> = specs
        .iter()
        .filter(|spec| match spec.catalog_visibility {
            CatalogVisibility::Public => true,
            CatalogVisibility::Authenticated => authenticated,
            CatalogVisibility::Hidden => false,
        })
        .collect();
    json!({
        "actions": visible.iter().filter(|spec| spec.transport.rest()).map(|spec| spec.name).collect::<Vec<_>>(),
        "mcp_only_actions": visible.iter().filter(|spec| spec.transport == ActionTransport::McpOnly).map(|spec| spec.name).collect::<Vec<_>>(),
        "catalog": rtemplate_contracts::actions::action_catalog_from(&visible),
        "preferred_rest_style": "direct_routes",
        "usage": "Use direct REST routes such as POST /v1/echo or GET /v1/status. MCP keeps a single action-dispatched tool; REST does not expose an action envelope.",
        "examples": {
            "greet":  {"method": "POST", "path": "/v1/greet",  "body": {"name": "Alice"}},
            "echo":   {"method": "POST", "path": "/v1/echo",   "body": {"message": "Hello!"}},
            "status": {"method": "GET", "path": "/v1/status"}
        }
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
        params
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("params must be an object"))?,
    )?;
    let text = required_string_param(params, "text")?;
    let reversed: String = text.chars().rev().collect();
    Ok(json!({"text": text, "reversed": reversed}))
}
