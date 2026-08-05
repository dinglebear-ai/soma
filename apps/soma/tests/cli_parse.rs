//! Integration tests for CLI argument parsing.
//!
//! **Customize**: extend these tests when you add new CLI subcommands.

use soma::cli::{Command, SelfUpdateCommand, SetupCommand, parse_args_from};

#[test]
fn test_greet_no_name_parsed() {
    assert_eq!(
        parse_args_from(["greet"]).unwrap(),
        Some(Command::Greet { name: None })
    );
}

#[test]
fn test_greet_with_name_parsed() {
    assert_eq!(
        parse_args_from(["greet", "--name", "Alice"]).unwrap(),
        Some(Command::Greet {
            name: Some("Alice".into())
        })
    );
}

#[test]
fn test_greet_rejects_flag_like_name_value() {
    let error = parse_args_from(["greet", "--name", "--bogus"]).unwrap_err();
    assert!(error.to_string().contains("requires a value after --name"));
}

#[test]
fn test_echo_message_parsed() {
    assert_eq!(
        parse_args_from(["echo", "--message", "Hello, World!"]).unwrap(),
        Some(Command::Echo {
            message: "Hello, World!".into()
        })
    );
}

#[test]
fn test_echo_no_message_is_rejected() {
    let error = parse_args_from(["echo"]).unwrap_err();
    assert!(error.to_string().contains("requires non-empty --message"));
}

#[test]
fn test_help_parsed() {
    assert_eq!(parse_args_from(["help"]).unwrap(), Some(Command::Help));
}

#[test]
fn test_watch_bad_interval_is_rejected() {
    let error = parse_args_from(["watch", "--interval", "nope"]).unwrap_err();
    assert!(error.to_string().contains("--interval"));
}

#[test]
fn test_setup_plugin_hook_no_repair_parsed() {
    assert_eq!(
        parse_args_from(["setup", "plugin-hook", "--no-repair"]).unwrap(),
        Some(Command::Setup(SetupCommand::PluginHook { no_repair: true }))
    );
}

#[test]
fn test_setup_check_parsed() {
    assert_eq!(
        parse_args_from(["setup", "check"]).unwrap(),
        Some(Command::Setup(SetupCommand::Check))
    );
}

#[test]
fn test_setup_repair_parsed() {
    assert_eq!(
        parse_args_from(["setup", "repair"]).unwrap(),
        Some(Command::Setup(SetupCommand::Repair))
    );
}

#[test]
fn test_setup_plugin_hook_default_parsed() {
    assert_eq!(
        parse_args_from(["setup", "plugin-hook"]).unwrap(),
        Some(Command::Setup(SetupCommand::PluginHook {
            no_repair: false
        }))
    );
}

#[test]
fn test_doctor_json_parsed() {
    assert_eq!(
        parse_args_from(["doctor", "--json"]).unwrap(),
        Some(Command::Doctor { json: true })
    );
}

#[test]
fn test_doctor_no_json_parsed() {
    assert_eq!(
        parse_args_from(["doctor"]).unwrap(),
        Some(Command::Doctor { json: false })
    );
}

#[test]
fn test_dynamic_provider_command_accepts_json_escape_hatch() {
    assert_eq!(
        parse_args_from(["weather", "--json", "{\"city\":\"Paris\"}"]).unwrap(),
        Some(Command::Provider {
            command: "weather".to_owned(),
            json: serde_json::json!({"city": "Paris"})
        })
    );
}

#[test]
fn test_package_generate_parsed() {
    assert_eq!(
        parse_args_from(["package", "generate", "--write"]).unwrap(),
        Some(Command::PackageGenerate { write: true })
    );
    assert_eq!(
        parse_args_from(["package", "generate", "--check"]).unwrap(),
        Some(Command::PackageGenerate { write: false })
    );
}

#[test]
fn test_dynamic_provider_command_accepts_flat_flags() {
    assert_eq!(
        parse_args_from(["weather", "--city", "Paris", "--days", "3"]).unwrap(),
        Some(Command::Provider {
            command: "weather".to_owned(),
            json: serde_json::json!({"city": "Paris", "days": 3})
        })
    );

    let error = parse_args_from(["weather", "--filters"]).unwrap_err();
    assert!(error.to_string().contains("--name value"));
}

#[test]
fn test_unknown_trailing_args_are_rejected() {
    for args in [
        &["status", "--bogus"][..],
        &["help", "--bogus"],
        &["greet", "--unknown"],
        &["echo", "--message", "hello", "--extra"],
        &["doctor", "--json", "--json"],
        &["watch", "--interval", "0"],
        &["setup", "plugin-hook", "--no-reapir"],
    ] {
        assert!(
            parse_args_from(args.iter().copied()).is_err(),
            "{args:?} should be rejected"
        );
    }
}

#[test]
fn test_self_update_run_parsed() {
    assert_eq!(
        parse_args_from([
            "self-update",
            "run",
            "--version",
            "2.0.0",
            "--url",
            "https://releases.example/soma",
            "--sha256",
            "aa",
        ])
        .unwrap(),
        Some(Command::SelfUpdate(SelfUpdateCommand::Run {
            version: "2.0.0".into(),
            url: "https://releases.example/soma".into(),
            sha256: "aa".into(),
            allow_http_loopback: false,
            state_file: None,
        }))
    );
}

#[test]
fn test_self_update_run_accepts_state_file_and_loopback() {
    assert_eq!(
        parse_args_from([
            "self-update",
            "run",
            "--version",
            "2.0.0",
            "--url",
            "http://127.0.0.1:8000/soma",
            "--sha256",
            "aa",
            "--state-file",
            "/var/lib/soma/update.json",
            "--allow-http-loopback",
        ])
        .unwrap(),
        Some(Command::SelfUpdate(SelfUpdateCommand::Run {
            version: "2.0.0".into(),
            url: "http://127.0.0.1:8000/soma".into(),
            sha256: "aa".into(),
            allow_http_loopback: true,
            state_file: Some("/var/lib/soma/update.json".into()),
        }))
    );
}

#[test]
fn test_self_update_recover_and_confirm_parsed() {
    assert_eq!(
        parse_args_from(["self-update", "recover"]).unwrap(),
        Some(Command::SelfUpdate(SelfUpdateCommand::Recover {
            state_file: None
        }))
    );
    assert_eq!(
        parse_args_from(["self-update", "confirm", "--state-file", "s.json"]).unwrap(),
        Some(Command::SelfUpdate(SelfUpdateCommand::Confirm {
            state_file: Some("s.json".into())
        }))
    );
}

#[test]
fn test_self_update_rejects_bad_invocations() {
    for args in [
        &["self-update"][..],
        &["self-update", "reinstall"],
        &["self-update", "run", "--version", "2.0.0"],
        &["self-update", "run", "--version", "2.0.0", "--url"],
        &["self-update", "recover", "--bogus"],
        &[
            "self-update",
            "run",
            "--version",
            "1",
            "--version",
            "2",
            "--url",
            "https://a",
            "--sha256",
            "aa",
        ],
    ] {
        assert!(
            parse_args_from(args.iter().copied()).is_err(),
            "{args:?} should be rejected"
        );
    }
}
