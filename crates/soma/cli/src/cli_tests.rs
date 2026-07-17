use soma_domain::{
    actions::{ActionCost, ActionSpec, ActionTransport, SomaAction, ACTION_SPECS},
    errors::ToolError,
};
use soma_provider_core::{
    CliOverlay, HostCapabilities, ProviderCatalog, ProviderIdentity, ProviderKind,
    ProviderManifest, ProviderTool,
};
use soma_test_support::{application_with_provider, default_application};

use super::{
    confirm_destructive_action_allowed, confirm_destructive_action_from_io, parse_args_from,
    provider_action_from_command, run, service_action_from_command, usage, CliIo, Command,
    ProviderCommand, SetupCommand,
};

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

#[derive(Default)]
struct TestIo {
    stdout: Vec<String>,
    stderr: Vec<String>,
}

impl CliIo for TestIo {
    fn stdout(&mut self, output: &str) -> anyhow::Result<()> {
        self.stdout.push(output.to_owned());
        Ok(())
    }

    fn stderr(&mut self, output: &str) -> anyhow::Result<()> {
        self.stderr.push(output.to_owned());
        Ok(())
    }

    fn confirm_destructive(&mut self, _action: &str) -> anyhow::Result<()> {
        Ok(())
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
fn parses_providers_list_with_dir_and_json() {
    let command = parse_args_from(["providers", "list", "--dir", "/tmp/providers", "--json"])
        .expect("parse command")
        .expect("command");

    assert_eq!(
        command,
        Command::Providers(ProviderCommand::List {
            dir: Some(std::path::PathBuf::from("/tmp/providers")),
            json: true,
        })
    );
}

#[test]
fn parses_providers_lint_with_dir_and_json() {
    let command = parse_args_from(["providers", "lint", "--dir", "/tmp/providers", "--json"])
        .expect("parse command")
        .expect("command");

    assert_eq!(
        command,
        Command::Providers(ProviderCommand::Lint {
            dir: Some(std::path::PathBuf::from("/tmp/providers")),
            json: true,
        })
    );
}

#[test]
fn parses_providers_status_with_dir_and_json() {
    let command = parse_args_from(["providers", "status", "--dir", "/tmp/providers", "--json"])
        .expect("parse command")
        .expect("command");

    assert_eq!(
        command,
        Command::Providers(ProviderCommand::Status {
            dir: Some(std::path::PathBuf::from("/tmp/providers")),
            json: true,
        })
    );
}

#[test]
fn parses_providers_dir_rejects_a_flag_token_as_the_value() {
    // `--dir --json` must not silently treat "--json" as a directory path —
    // that would make an inspection of a nonexistent directory (which is a
    // valid, empty, zero-invalid report) look like a clean lint run.
    let error =
        parse_args_from(["providers", "lint", "--dir", "--json"]).expect_err("missing --dir value");
    assert!(error.to_string().contains("--dir requires a value"));
}

#[test]
fn parses_providers_dir_rejects_missing_trailing_value() {
    let error = parse_args_from(["providers", "lint", "--dir"]).expect_err("missing --dir value");
    assert!(error.to_string().contains("--dir requires a value"));
}

#[test]
fn providers_validate_and_inspect_parse_as_management_commands() {
    assert_eq!(
        parse_args_from(["providers", "validate"]).unwrap().unwrap(),
        Command::Providers(ProviderCommand::Validate)
    );
    assert_eq!(
        parse_args_from(["providers", "inspect"]).unwrap().unwrap(),
        Command::Providers(ProviderCommand::Inspect)
    );
}

#[test]
fn providers_test_accepts_optional_json_payload() {
    assert_eq!(
        parse_args_from(["providers", "test", "weather-current"])
            .unwrap()
            .unwrap(),
        Command::Providers(ProviderCommand::Test {
            action: "weather-current".to_owned(),
            json: serde_json::json!({})
        })
    );
    assert_eq!(
        parse_args_from([
            "providers",
            "test",
            "weather-current",
            "--json",
            "{\"city\":\"Paris\"}",
        ])
        .unwrap()
        .unwrap(),
        Command::Providers(ProviderCommand::Test {
            action: "weather-current".to_owned(),
            json: serde_json::json!({"city": "Paris"})
        })
    );
}

#[test]
fn dynamic_provider_command_resolves_cli_command_and_alias_to_action() {
    let application = application_with_provider(provider_catalog(), serde_json::json!({}));

    assert_eq!(
        provider_action_from_command(
            &Command::Provider {
                command: "forecast".to_owned(),
                json: serde_json::json!({})
            },
            &application
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
            &application
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
        Some(SomaAction::Greet {
            name: Some("Alice".into())
        })
    );
    assert_eq!(
        service_action_from_command(&Command::Echo {
            message: "hello".into()
        }),
        Some(SomaAction::Echo {
            message: "hello".into()
        })
    );
    assert_eq!(
        service_action_from_command(&Command::Status),
        Some(SomaAction::Status)
    );
    assert_eq!(
        service_action_from_command(&Command::Help),
        Some(SomaAction::Help)
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
    assert_eq!(
        service_action_from_command(&Command::Providers(ProviderCommand::Validate)),
        None
    );
}

#[tokio::test]
async fn run_dynamic_command_uses_application_dispatch() {
    let application = application_with_provider(provider_catalog(), serde_json::json!({}));
    let mut io = TestIo::default();
    run(
        application,
        Command::Provider {
            command: "forecast".to_owned(),
            json: serde_json::json!({}),
        }
        .into(),
        &mut io,
    )
    .await
    .expect("dynamic command should run through the application facade");
    assert_eq!(io.stdout, vec!["{}"]);
    assert!(io.stderr.is_empty());
}

// ── run() coverage for the built-in service actions (greet/echo/status/help) ──
//
// `run_dynamic_command_uses_application_dispatch` above only exercises the
// dynamic Provider command path. These tests drive `run()` for every
// service-backed `Command` variant and assert on the *serialized stdout*
// `run()` actually prints — pinning the CLI's own JSON rendering of
// `SomaApplication::execute_action`'s `ExecuteActionResponse` so a shape
// change during the PR12/13 soma-application/soma-contracts split fails a
// test instead of silently altering `soma greet`/`echo`/`status`/`help`.

#[tokio::test]
async fn run_greet_command_prints_default_greeting_json() {
    let application = default_application();
    let mut io = TestIo::default();
    run(application, Command::Greet { name: None }.into(), &mut io)
        .await
        .expect("greet should run through the application facade");

    let expected = serde_json::to_string_pretty(&serde_json::json!({
        "greeting": "Hello, World!",
        "target": "World",
        "server": "",
    }))
    .unwrap();
    assert_eq!(io.stdout, vec![expected]);
    assert!(io.stderr.is_empty());
}

#[tokio::test]
async fn run_greet_command_with_name_prints_personalized_greeting_json() {
    let application = default_application();
    let mut io = TestIo::default();
    run(
        application,
        Command::Greet {
            name: Some("Alice".into()),
        }
        .into(),
        &mut io,
    )
    .await
    .expect("greet --name should run through the application facade");

    let expected = serde_json::to_string_pretty(&serde_json::json!({
        "greeting": "Hello, Alice!",
        "target": "Alice",
        "server": "",
    }))
    .unwrap();
    assert_eq!(io.stdout, vec![expected]);
    assert!(io.stderr.is_empty());
}

#[tokio::test]
async fn run_echo_command_prints_message_json() {
    let application = default_application();
    let mut io = TestIo::default();
    run(
        application,
        Command::Echo {
            message: "hello".into(),
        }
        .into(),
        &mut io,
    )
    .await
    .expect("echo should run through the application facade");

    let expected = serde_json::to_string_pretty(&serde_json::json!({ "echo": "hello" })).unwrap();
    assert_eq!(io.stdout, vec![expected]);
    assert!(io.stderr.is_empty());
}

#[tokio::test]
async fn run_status_command_prints_status_json() {
    // Suppress the stale-binary warning field so the snapshot is stable
    // regardless of source/binary mtimes in the build environment. Save and
    // restore any prior value — `cargo test` runs tests in this binary on
    // shared threads, so an unrestored `set_var` would leak into whichever
    // other test happens to run next.
    const VAR: &str = "SOMA_SUPPRESS_STALE_BINARY_WARNING";
    let previous = std::env::var(VAR).ok();
    std::env::set_var(VAR, "1");

    let application = default_application();
    let mut io = TestIo::default();
    let result = run(application, Command::Status.into(), &mut io).await;

    match previous {
        Some(value) => std::env::set_var(VAR, value),
        None => std::env::remove_var(VAR),
    }

    result.expect("status should run through the application facade");

    let expected = serde_json::to_string_pretty(&serde_json::json!({
        "status": "ok",
        "note": "stub — replace with real health endpoint",
    }))
    .unwrap();
    assert_eq!(io.stdout, vec![expected]);
    assert!(io.stderr.is_empty());
}

#[tokio::test]
async fn run_help_command_prints_action_reference_json() {
    let application = default_application();
    let mut io = TestIo::default();
    run(application, Command::Help.into(), &mut io)
        .await
        .expect("help should run through the application facade");

    assert_eq!(io.stdout.len(), 1);
    let result: serde_json::Value = serde_json::from_str(&io.stdout[0]).unwrap();

    assert_eq!(result["preferred_rest_style"], "direct_routes");
    assert_eq!(
        result["usage"],
        "Use direct REST routes such as POST /v1/echo or GET /v1/status. \
MCP keeps a single action-dispatched tool; REST does not expose an action envelope."
    );
    assert_eq!(
        result["examples"],
        serde_json::json!({
            "greet":  {"method": "POST", "path": "/v1/greet",  "body": {"name": "Alice"}},
            "echo":   {"method": "POST", "path": "/v1/echo",   "body": {"message": "Hello!"}},
            "status": {"method": "GET", "path": "/v1/status"},
        })
    );
    let actions: Vec<&str> = result["actions"]
        .as_array()
        .expect("actions should be an array")
        .iter()
        .map(|v| v.as_str().expect("action name should be a string"))
        .collect();
    for expected in ["greet", "echo", "status", "help"] {
        assert!(
            actions.contains(&expected),
            "help actions missing `{expected}`: {actions:?}"
        );
    }
    assert!(io.stderr.is_empty());
}

#[test]
fn usage_mentions_current_cli_commands_and_loopback_default() {
    let text = usage();
    for expected in [
        "soma help",
        "soma doctor",
        "soma setup plugin-hook",
        "soma providers validate",
        "soma watch",
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
        &["providers"],
        &["providers", "validate", "--json"],
        &["providers", "test"],
        &["providers", "test", "weather-current", "--json"],
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
