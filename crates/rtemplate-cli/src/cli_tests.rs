use std::sync::Arc;

use async_trait::async_trait;
use rtemplate_contracts::errors::ToolError;
use rtemplate_contracts::{
    actions::{ActionCost, ActionSpec, ActionTransport, ExampleAction, ACTION_SPECS},
    providers::{
        CliOverlay, HostCapabilities, ProviderCatalog, ProviderIdentity, ProviderKind,
        ProviderManifest, ProviderTool,
    },
};
use rtemplate_service::{
    provider_registry::{Provider, ProviderCall, ProviderOutput, ProviderRegistry},
    ProviderError,
};

use super::{
    confirm_destructive_action_allowed, confirm_destructive_action_from_io, parse_args_from,
    provider_action_from_command, run, service_action_from_command, usage, Command, SetupCommand,
};
use rtemplate_contracts::config::ExampleConfig;

const TEST_DESTRUCTIVE_ACTIONS: &[ActionSpec] = &[ActionSpec {
    name: "delete_everything",
    description: "Delete everything.",
    required_scope: None,
    transport: ActionTransport::Any,
    rest_method: None,
    rest_path: None,
    destructive: true,
    requires_admin: false,
    cost: ActionCost::Write,
    params: &[],
    returns: "DeleteResult",
    cli: None,
}];

#[derive(Clone)]
struct CliProvider {
    catalog: ProviderCatalog,
}

#[async_trait]
impl Provider for CliProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::json(serde_json::json!({})))
    }
}

fn provider_catalog() -> ProviderCatalog {
    ProviderManifest {
        schema_version: 1,
        provider: ProviderIdentity {
            name: "weather".to_owned(),
            kind: ProviderKind::StaticRust,
            title: None,
            description: None,
            homepage: None,
            source: None,
            version: None,
            enabled: Some(true),
        },
        tools: vec![ProviderTool {
            name: "weather-current".to_owned(),
            description: "Fetch current weather.".to_owned(),
            title: None,
            input_schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
            output_schema: None,
            scope: None,
            destructive: false,
            requires_admin: false,
            cost: None,
            env: Vec::new(),
            limits: None,
            mcp: None,
            rest: None,
            cli: Some(CliOverlay {
                enabled: true,
                command: Some("forecast".to_owned()),
                aliases: vec!["wx".to_owned()],
                about: None,
                long_about: None,
                hidden: false,
                flags: Vec::new(),
                default_output: None,
                interactive: false,
            }),
            palette: None,
            ui: None,
            examples: Vec::new(),
            meta: serde_json::json!({}),
        }],
        prompts: Vec::new(),
        resources: Vec::new(),
        tasks: Vec::new(),
        elicitation: Vec::new(),
        env: Vec::new(),
        capabilities: HostCapabilities::default(),
        docs: None,
        plugin: None,
        ui: None,
        meta: serde_json::json!({}),
    }
}

#[test]
fn empty_args_returns_none() {
    let result = parse_args_from::<_, String>([]).unwrap();
    assert!(result.is_none());
}

#[test]
fn unknown_subcommand_becomes_dynamic_provider_command() {
    let result = parse_args_from(["unknown-command"]).unwrap();
    assert_eq!(
        result,
        Some(Command::Provider {
            command: "unknown-command".to_owned(),
            json: serde_json::json!({})
        })
    );
}

#[test]
fn dynamic_provider_command_accepts_flat_flags_without_json() {
    let result = parse_args_from(["weather-current", "--city", "Paris", "--units", "metric"])
        .unwrap()
        .unwrap();
    assert_eq!(
        result,
        Command::Provider {
            command: "weather-current".to_owned(),
            json: serde_json::json!({
                "city": "Paris",
                "units": "metric"
            })
        }
    );
}

#[test]
fn dynamic_provider_command_resolves_cli_command_and_alias_to_action() {
    let registry = ProviderRegistry::new(vec![Arc::new(CliProvider {
        catalog: provider_catalog(),
    })])
    .expect("registry");

    assert_eq!(
        provider_action_from_command(
            &Command::Provider {
                command: "forecast".to_owned(),
                json: serde_json::json!({})
            },
            &registry
        )
        .unwrap(),
        "weather-current"
    );
    assert_eq!(
        provider_action_from_command(
            &Command::Provider {
                command: "wx".to_owned(),
                json: serde_json::json!({})
            },
            &registry
        )
        .unwrap(),
        "weather-current"
    );
}

#[test]
fn greet_no_name() {
    let cmd = parse_args_from(["greet"]).unwrap().unwrap();
    assert_eq!(cmd, Command::Greet { name: None });
}

#[test]
fn greet_with_name_flag() {
    let cmd = parse_args_from(["greet", "--name", "Alice"])
        .unwrap()
        .unwrap();
    assert_eq!(
        cmd,
        Command::Greet {
            name: Some("Alice".into())
        }
    );
}

#[test]
fn echo_with_message_flag() {
    let cmd = parse_args_from(["echo", "--message", "hello"])
        .unwrap()
        .unwrap();
    assert_eq!(
        cmd,
        Command::Echo {
            message: "hello".into()
        }
    );
}

#[test]
fn echo_missing_message_is_error() {
    let err = parse_args_from(["echo"]).unwrap_err();
    assert!(err.to_string().contains("--message"));
}

#[test]
fn status_subcommand() {
    let cmd = parse_args_from(["status"]).unwrap().unwrap();
    assert_eq!(cmd, Command::Status);
}

#[test]
fn help_subcommand() {
    let cmd = parse_args_from(["help"]).unwrap().unwrap();
    assert_eq!(cmd, Command::Help);
}

#[test]
fn service_commands_convert_to_shared_actions() {
    assert_eq!(
        service_action_from_command(&Command::Greet {
            name: Some("Alice".into())
        }),
        Some(ExampleAction::Greet {
            name: Some("Alice".into())
        })
    );
    assert_eq!(
        service_action_from_command(&Command::Echo {
            message: "hello".into()
        }),
        Some(ExampleAction::Echo {
            message: "hello".into()
        })
    );
    assert_eq!(
        service_action_from_command(&Command::Status),
        Some(ExampleAction::Status)
    );
    assert_eq!(
        service_action_from_command(&Command::Help),
        Some(ExampleAction::Help)
    );
}

#[test]
fn cli_parser_covers_every_cli_action_in_registry() {
    for spec in ACTION_SPECS.iter().filter(|spec| spec.cli.is_some()) {
        let cli = spec.cli.unwrap();
        let args = match spec.name {
            "greet" => vec![cli.command],
            "echo" => vec![cli.command, "--message", "hello"],
            "status" | "help" => vec![cli.command],
            other => panic!("add a parser parity fixture for action `{other}`"),
        };
        let command = parse_args_from(args)
            .unwrap()
            .unwrap_or_else(|| panic!("registered CLI action `{}` did not parse", spec.name));
        let action = service_action_from_command(&command)
            .unwrap_or_else(|| panic!("registered CLI action `{}` did not dispatch", spec.name));
        assert_eq!(action.name(), spec.name);
    }
}

#[test]
fn cli_error_format_uses_shared_tool_error_fields() {
    let error = ToolError::validation("missing_field", "`message` is required", "Provide it")
        .with_field("message");
    let rendered = super::format_cli_tool_error(&error);
    assert!(rendered.contains("code: missing_field"));
    assert!(rendered.contains("kind: validation"));
    assert!(rendered.contains("field: message"));
    assert!(rendered.contains("remediation: Provide it"));
}

#[test]
fn doctor_no_flags() {
    let cmd = parse_args_from(["doctor"]).unwrap().unwrap();
    assert_eq!(cmd, Command::Doctor { json: false });
}

#[test]
fn doctor_json_flag() {
    let cmd = parse_args_from(["doctor", "--json"]).unwrap().unwrap();
    assert_eq!(cmd, Command::Doctor { json: true });
}

#[test]
fn watch_defaults() {
    let cmd = parse_args_from(["watch"]).unwrap().unwrap();
    assert_eq!(
        cmd,
        Command::Watch {
            url: None,
            interval: 10
        }
    );
}

#[test]
fn watch_with_url_and_interval() {
    let cmd = parse_args_from([
        "watch",
        "--url",
        "http://localhost:40060",
        "--interval",
        "5",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(
        cmd,
        Command::Watch {
            url: Some("http://localhost:40060".into()),
            interval: 5
        }
    );
}

#[test]
fn setup_check() {
    let cmd = parse_args_from(["setup", "check"]).unwrap().unwrap();
    assert_eq!(cmd, Command::Setup(SetupCommand::Check));
}

#[test]
fn setup_repair() {
    let cmd = parse_args_from(["setup", "repair"]).unwrap().unwrap();
    assert_eq!(cmd, Command::Setup(SetupCommand::Repair));
}

#[test]
fn setup_plugin_hook() {
    let cmd = parse_args_from(["setup", "plugin-hook"]).unwrap().unwrap();
    assert_eq!(
        cmd,
        Command::Setup(SetupCommand::PluginHook { no_repair: false })
    );
}

#[test]
fn setup_plugin_hook_no_repair_flag() {
    let cmd = parse_args_from(["setup", "plugin-hook", "--no-repair"])
        .unwrap()
        .unwrap();
    assert_eq!(
        cmd,
        Command::Setup(SetupCommand::PluginHook { no_repair: true })
    );
}

#[test]
fn operational_commands_do_not_convert_to_service_actions() {
    assert_eq!(
        service_action_from_command(&Command::Doctor { json: true }),
        None
    );
    assert_eq!(
        service_action_from_command(&Command::Watch {
            url: None,
            interval: 10
        }),
        None
    );
    assert_eq!(
        service_action_from_command(&Command::Setup(SetupCommand::Check)),
        None
    );
}

#[tokio::test]
async fn run_service_command_uses_shared_dispatch_path() {
    run(Command::Status, &ExampleConfig::default())
        .await
        .expect("status should run through shared service dispatch");
}

#[test]
fn usage_mentions_current_cli_commands_and_loopback_default() {
    let text = usage();
    for expected in [
        "example help",
        "example doctor",
        "example setup plugin-hook",
        "example watch",
        "default 127.0.0.1",
    ] {
        assert!(text.contains(expected), "usage missing {expected}");
    }
}

#[test]
fn parser_rejects_unknown_and_malformed_flags() {
    for args in [
        &["status", "--bogus"][..],
        &["help", "--bogus"],
        &["greet", "--bogus"],
        &["greet", "--name"],
        &["greet", "--name", "--bogus"],
        &["greet", "--name", "Alice", "extra"],
        &["doctor", "--bogus"],
        &["doctor", "--json", "--json"],
        &["watch", "--url", "http://localhost:40060", "--bogus"],
        &["watch", "--interval", "0"],
        &["setup", "check", "--no-repair"],
        &["setup", "plugin-hook", "--no-reapir"],
    ] {
        assert!(
            parse_args_from(args.iter().copied()).is_err(),
            "{args:?} should be rejected"
        );
    }
}

#[test]
fn parser_reports_duplicate_value_flags() {
    let err = parse_args_from(["greet", "--name", "Alice", "--name", "Bob"]).unwrap_err();
    assert!(err.to_string().contains("duplicate --name"));
}

#[test]
fn destructive_confirmation_is_not_required_for_safe_actions() {
    confirm_destructive_action_allowed(TEST_DESTRUCTIVE_ACTIONS, "status", false, false).unwrap();
}

#[test]
fn destructive_confirmation_requires_yes_when_non_interactive() {
    let err = confirm_destructive_action_allowed(
        TEST_DESTRUCTIVE_ACTIONS,
        "delete_everything",
        false,
        false,
    )
    .unwrap_err();
    assert!(err.to_string().contains("--yes"));

    confirm_destructive_action_allowed(TEST_DESTRUCTIVE_ACTIONS, "delete_everything", true, false)
        .unwrap();
}

#[test]
fn destructive_confirmation_accepts_exact_action_name() {
    let mut input = std::io::Cursor::new(b"delete_everything\n");
    let mut output = Vec::new();
    confirm_destructive_action_from_io("delete_everything", &mut input, &mut output).unwrap();
    let prompt = String::from_utf8(output).unwrap();
    assert!(prompt.contains("delete_everything"));
}

#[test]
fn destructive_confirmation_rejects_mismatched_input() {
    let mut input = std::io::Cursor::new(b"nope\n");
    let mut output = Vec::new();
    let err = confirm_destructive_action_from_io("delete_everything", &mut input, &mut output)
        .unwrap_err();
    assert!(err.to_string().contains("aborted"));
}
