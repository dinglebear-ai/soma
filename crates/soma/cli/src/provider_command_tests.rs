use super::{ProviderCommand, parse_providers_command};
use crate::Command;

#[test]
fn parses_validate_inspect_and_test() {
    assert_eq!(
        parse_providers_command(&["validate".to_owned()]).unwrap(),
        Command::Providers(ProviderCommand::Validate)
    );
    assert_eq!(
        parse_providers_command(&["inspect".to_owned()]).unwrap(),
        Command::Providers(ProviderCommand::Inspect)
    );
    assert_eq!(
        parse_providers_command(&["test".to_owned(), "greet".to_owned()]).unwrap(),
        Command::Providers(ProviderCommand::Test {
            action: "greet".to_owned(),
            json: serde_json::json!({}),
        })
    );
}

#[test]
fn parses_list_lint_status_with_dir_and_json() {
    for action in ["list", "lint", "status"] {
        let command = parse_providers_command(&[
            action.to_owned(),
            "--dir".to_owned(),
            "/tmp/providers".to_owned(),
            "--json".to_owned(),
        ])
        .unwrap();
        let Command::Providers(provider_command) = command else {
            panic!("expected Command::Providers");
        };
        assert!(provider_command.is_non_executing());
    }
}

#[test]
fn rejects_a_flag_token_as_the_dir_value() {
    let error =
        parse_providers_command(&["lint".to_owned(), "--dir".to_owned(), "--json".to_owned()])
            .unwrap_err();
    assert!(error.to_string().contains("--dir requires a value"));
}

#[test]
fn rejects_empty_and_unknown_subcommands() {
    assert!(parse_providers_command(&[]).is_err());
    assert!(parse_providers_command(&["bogus".to_owned()]).is_err());
}

#[test]
fn only_the_three_filesystem_variants_are_non_executing() {
    assert!(!ProviderCommand::Validate.is_non_executing());
    assert!(!ProviderCommand::Inspect.is_non_executing());
    assert!(
        !ProviderCommand::Test {
            action: "greet".to_owned(),
            json: serde_json::json!({}),
        }
        .is_non_executing()
    );
    assert!(
        ProviderCommand::List {
            dir: None,
            json: false
        }
        .is_non_executing()
    );
    assert!(
        ProviderCommand::Lint {
            dir: None,
            json: false
        }
        .is_non_executing()
    );
    assert!(
        ProviderCommand::Status {
            dir: None,
            json: false
        }
        .is_non_executing()
    );
}

#[test]
fn parses_all_graduation_commands_and_optional_inputs() {
    assert_eq!(
        parse_providers_command(&[
            "graduate".to_owned(),
            "--source".to_owned(),
            "provider.py".to_owned(),
            "--workspace".to_owned(),
            "graduated".to_owned(),
            "--fixtures".to_owned(),
            "fixtures.json".to_owned(),
        ])
        .unwrap(),
        Command::Providers(ProviderCommand::Graduate {
            source: "provider.py".into(),
            workspace: "graduated".into(),
            fixtures: Some("fixtures.json".into()),
        })
    );
    assert_eq!(
        parse_providers_command(&[
            "build-component".to_owned(),
            "--workspace".to_owned(),
            "graduated".to_owned(),
        ])
        .unwrap(),
        Command::Providers(ProviderCommand::BuildComponent {
            workspace: "graduated".into(),
            component: None,
        })
    );
    assert_eq!(
        parse_providers_command(&[
            "verify-component".to_owned(),
            "--component".to_owned(),
            "provider.wasm".to_owned(),
        ])
        .unwrap(),
        Command::Providers(ProviderCommand::VerifyComponent {
            component: "provider.wasm".into(),
        })
    );
    assert_eq!(
        parse_providers_command(&[
            "compare".to_owned(),
            "--component".to_owned(),
            "provider.wasm".to_owned(),
            "--fixtures".to_owned(),
            "fixtures.json".to_owned(),
        ])
        .unwrap(),
        Command::Providers(ProviderCommand::Compare {
            component: "provider.wasm".into(),
            fixtures: "fixtures.json".into(),
        })
    );
    for action in ["activate", "rollback"] {
        let command = parse_providers_command(&[
            action.to_owned(),
            "--workspace".to_owned(),
            "graduated".to_owned(),
        ])
        .unwrap();
        assert!(matches!(
            command,
            Command::Providers(ProviderCommand::Activate { .. })
                | Command::Providers(ProviderCommand::Rollback { .. })
        ));
    }
}

#[test]
fn graduation_commands_reject_duplicates_unknowns_and_missing_values() {
    for args in [
        vec!["graduate", "--source", "provider.py"],
        vec![
            "graduate",
            "--source",
            "provider.py",
            "--source",
            "other.py",
        ],
        vec!["build-component", "--workspace", "--component"],
        vec!["activate", "--bogus", "workspace"],
    ] {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert!(parse_providers_command(&args).is_err(), "{args:?}");
    }
}
