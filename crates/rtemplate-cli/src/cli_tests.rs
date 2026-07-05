use rtemplate_contracts::actions::{ActionCost, ActionSpec, ActionTransport, CatalogVisibility};
use rtemplate_contracts::errors::ToolError;
use serde_json::json;

use super::{
    confirm_destructive_action_allowed, confirm_destructive_action_from_io, parse_args_from, run,
    usage, Command, SetupCommand,
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
    catalog_visibility: CatalogVisibility::Hidden,
}];

#[test]
fn empty_args_returns_none() {
    let result = parse_args_from::<_, String>([]).unwrap();
    assert!(result.is_none());
}

#[test]
fn unknown_subcommand_returns_none() {
    let result = parse_args_from(["unknown-command"]).unwrap();
    assert!(result.is_none());
}

#[test]
fn greet_no_name() {
    let cmd = parse_args_from(["greet"]).unwrap().unwrap();
    assert_eq!(
        cmd,
        Command::Action {
            name: "greet".to_owned(),
            params: json!({}),
            yes: false,
        }
    );
}

#[test]
fn greet_with_name_flag() {
    let cmd = parse_args_from(["greet", "--name", "Alice"])
        .unwrap()
        .unwrap();
    assert_eq!(
        cmd,
        Command::Action {
            name: "greet".to_owned(),
            params: json!({"name": "Alice"}),
            yes: false,
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
        Command::Action {
            name: "echo".to_owned(),
            params: json!({"message": "hello"}),
            yes: false,
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
    assert_eq!(
        cmd,
        Command::Action {
            name: "status".to_owned(),
            params: json!({}),
            yes: false,
        }
    );
}

#[test]
fn help_subcommand() {
    let cmd = parse_args_from(["help"]).unwrap().unwrap();
    assert_eq!(
        cmd,
        Command::Action {
            name: "help".to_owned(),
            params: json!({}),
            yes: false,
        }
    );
}

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
    assert!(error
        .to_string()
        .contains("missing required flag --message"));
}

#[test]
fn cli_parser_covers_every_cli_action_in_registry() {
    for spec in rtemplate_service::action_specs()
        .iter()
        .filter(|spec| spec.cli.is_some())
    {
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
        let Command::Action { name, .. } = command else {
            panic!(
                "registered CLI action `{}` parsed to non-action command",
                spec.name
            );
        };
        assert_eq!(name, spec.name);
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
fn operational_commands_parse_outside_service_action_path() {
    let doctor = parse_args_from(["doctor", "--json"]).unwrap().unwrap();
    assert!(matches!(doctor, Command::Doctor { json: true }));

    let watch = parse_args_from(["watch", "--url", "http://localhost:40060"])
        .unwrap()
        .unwrap();
    assert!(matches!(watch, Command::Watch { .. }));

    let setup = parse_args_from(["setup", "check"]).unwrap().unwrap();
    assert!(matches!(setup, Command::Setup(SetupCommand::Check)));
}

#[test]
fn dynamic_action_rejects_single_dash_flag_looking_values() {
    let err = parse_args_from(["echo", "--message", "-y"]).unwrap_err();
    assert!(err.to_string().contains("value looks like a flag"));
}

#[tokio::test]
async fn run_service_command_uses_shared_dispatch_path() {
    run(
        Command::Action {
            name: "status".to_owned(),
            params: json!({}),
            yes: false,
        },
        &ExampleConfig::default(),
    )
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
    assert!(err.to_string().contains("duplicate flag --name"));
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
