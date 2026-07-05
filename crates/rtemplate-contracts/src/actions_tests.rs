use serde_json::json;

use super::*;

#[test]
fn action_metadata_is_the_action_source_of_truth() {
    assert_eq!(
        action_names(),
        vec![
            "greet",
            "echo",
            "status",
            "elicit_name",
            "scaffold_intent",
            "help"
        ]
    );
    assert_eq!(rest_action_names(), vec!["greet", "echo", "status", "help"]);
    assert_eq!(cli_action_names(), vec!["greet", "echo", "status", "help"]);
    assert_eq!(cli_commands(), vec!["greet", "echo", "status", "help"]);
    assert_eq!(
        mcp_only_action_names(),
        vec!["elicit_name", "scaffold_intent"]
    );
    assert!(is_rest_action("greet"));
    assert!(!is_rest_action("scaffold_intent"));
    assert_eq!(required_scope_for_action("help"), None);
    assert_eq!(required_scope_for_action("greet"), Some(READ_SCOPE));
    assert_eq!(required_scope_for_action("unknown"), Some(DENY_SCOPE));
    let echo = action_spec("echo").expect("echo spec should exist");
    assert_eq!(echo.description, "Echo a message back unchanged.");
    assert_eq!(echo.returns, "EchoResult");
    assert_eq!(echo.cost, ActionCost::Cheap);
    assert_eq!(echo.params[0].name, "message");
    assert!(echo.params[0].required);
    assert_eq!(echo.cli.unwrap().command, "echo");
    assert_eq!(echo.cli.unwrap().flags[0].name, "--message");
    assert!(echo.cli.unwrap().flags[0].required);
    assert!(!echo.destructive);
    assert!(!echo.requires_admin);
}

#[test]
fn action_catalog_projects_surfaces_and_auth_posture() {
    let catalog = action_catalog();
    let greet = catalog
        .iter()
        .find(|entry| entry.action == "greet")
        .expect("greet catalog entry should exist");
    assert_eq!(greet.service, "example");
    assert!(greet.surface_availability.mcp);
    assert!(greet.surface_availability.cli);
    assert!(greet.surface_availability.rest);
    assert!(!greet.surface_availability.web_ui);
    assert_eq!(greet.required_scope.as_deref(), Some(READ_SCOPE));
    assert_eq!(greet.cost, "cheap");
    let name_param = greet
        .params
        .iter()
        .find(|param| param.name == "name")
        .expect("greet should document name param");
    assert_eq!(name_param.max_len, Some(4096));
    assert!(name_param.enum_values.is_empty());
    assert_eq!(greet.cli.as_ref().unwrap().command, "greet");
    assert_eq!(
        greet.cli.as_ref().unwrap().usage,
        "example greet [--name NAME]"
    );
    assert!(greet.auth_posture.contains(READ_SCOPE));

    let scaffold = catalog
        .iter()
        .find(|entry| entry.action == "scaffold_intent")
        .expect("scaffold_intent catalog entry should exist");
    assert!(scaffold.surface_availability.mcp);
    assert!(!scaffold.surface_availability.cli);
    assert!(!scaffold.surface_availability.rest);
    assert!(scaffold.cli.is_none());
    assert!(scaffold.mcp_only_exception.is_some());
}

#[test]
fn every_cli_action_has_cli_metadata_and_every_mcp_only_action_does_not() {
    for spec in ACTION_SPECS {
        if spec.transport.cli() {
            let cli = spec
                .cli
                .unwrap_or_else(|| panic!("{} should declare CLI metadata", spec.name));
            assert_eq!(
                cli.command, spec.name,
                "{} CLI command should match the action name unless the template explicitly adds an alias map",
                spec.name
            );
            assert!(
                cli.usage.contains(cli.command),
                "{} CLI usage should mention its command",
                spec.name
            );
        } else {
            assert!(
                spec.cli.is_none(),
                "{} should not advertise CLI metadata when transport excludes CLI",
                spec.name
            );
        }
    }
}

#[test]
fn mcp_args_parse_flat_shape() {
    let action = ExampleAction::from_mcp_args(&json!({ "action": "echo", "message": "hello" }))
        .expect("flat MCP args should parse");
    assert_eq!(
        action,
        ExampleAction::Echo {
            message: "hello".into()
        }
    );
}

#[test]
fn rest_args_parse_nested_params_shape() {
    let action = ExampleAction::from_rest("greet", &json!({ "name": "Alice" }))
        .expect("REST params should parse");
    assert_eq!(
        action,
        ExampleAction::Greet {
            name: Some("Alice".into())
        }
    );
}

#[test]
fn missing_action_is_validation_error() {
    let error = ExampleAction::from_mcp_args(&json!({})).unwrap_err();
    assert!(error.to_string().contains("action is required"));
    let legacy_validation = error
        .downcast_ref::<ValidationError>()
        .expect("parser errors should remain downcast-compatible as ValidationError");
    assert_eq!(legacy_validation.code(), "missing_action");
    let validation = action_validation_error(&error).expect("error should classify as validation");
    assert_eq!(validation.code(), "missing_action");
    assert_eq!(validation.field(), Some("action"));
    assert!(validation.remediation().contains("action=help"));
}

#[test]
fn non_string_action_is_wrong_type_validation_error() {
    let error = ExampleAction::from_mcp_args(&json!({ "action": 42 })).unwrap_err();
    assert!(error.to_string().contains("`action` must be a string"));
    let validation = error
        .downcast_ref::<ValidationError>()
        .expect("parser errors should remain downcast-compatible as ValidationError");
    assert_eq!(validation.code(), "wrong_type");
    assert_eq!(validation.field(), Some("action"));
}

#[test]
fn echo_rejects_missing_and_empty_message() {
    let missing = ExampleAction::from_mcp_args(&json!({ "action": "echo" })).unwrap_err();
    assert!(missing.to_string().contains("`message` is required"));
    let validation =
        action_validation_error(&missing).expect("error should classify as validation");
    assert_eq!(validation.code(), "missing_field");
    assert_eq!(validation.field(), Some("message"));

    let empty = ExampleAction::from_rest("echo", &json!({ "message": "" })).unwrap_err();
    assert!(empty.to_string().contains("`message` is required"));
}

#[test]
fn string_params_reject_wrong_json_type() {
    let greet = ExampleAction::from_rest("greet", &json!({ "name": 42 })).unwrap_err();
    assert!(greet.to_string().contains("`name` must be a string"));

    let echo = ExampleAction::from_mcp_args(&json!({
        "action": "echo",
        "message": ["not", "a", "string"]
    }))
    .unwrap_err();
    assert!(echo.to_string().contains("`message` must be a string"));
    let validation = action_validation_error(&echo).expect("error should classify as validation");
    assert_eq!(validation.code(), "wrong_type");
    assert_eq!(validation.field(), Some("message"));
}

#[test]
fn scaffold_intent_parses_as_mcp_only_action() {
    let action = ExampleAction::from_mcp_args(&json!({ "action": "scaffold_intent" }))
        .expect("scaffold_intent should parse");
    assert_eq!(action, ExampleAction::ScaffoldIntent);
}

#[test]
fn rest_rejects_mcp_only_actions() {
    let error = ExampleAction::from_rest("scaffold_intent", &json!({})).unwrap_err();
    assert!(error.to_string().contains("not available over REST"));

    let error = ExampleAction::from_rest("elicit_name", &json!({})).unwrap_err();
    assert!(error.to_string().contains("not available over REST"));
}

#[test]
fn rest_missing_action_preserves_missing_action_error() {
    let error = ExampleAction::from_rest("", &json!({})).unwrap_err();
    assert_eq!(error.to_string(), "action is required");
}

#[test]
fn unknown_action_mentions_help() {
    let error = ExampleAction::from_rest("missing", &json!({})).unwrap_err();
    assert!(error.to_string().contains("action=help"));
    let validation = action_validation_error(&error).expect("error should classify as validation");
    assert_eq!(validation.code(), "unknown_action");
    assert_eq!(validation.bad_value(), Some("missing"));
}

#[test]
fn all_parse_errors_are_classified_as_validation_errors() {
    let cases: &[anyhow::Error] = &[
        ExampleAction::from_mcp_args(&json!({})).unwrap_err(),
        ExampleAction::from_mcp_args(&json!({ "action": "echo" })).unwrap_err(),
        ExampleAction::from_rest("echo", &json!({ "message": "" })).unwrap_err(),
        ExampleAction::from_rest("greet", &json!({ "name": 42 })).unwrap_err(),
        ExampleAction::from_rest("scaffold_intent", &json!({})).unwrap_err(),
        ExampleAction::from_rest("missing", &json!({})).unwrap_err(),
    ];
    for (i, err) in cases.iter().enumerate() {
        assert!(
            is_validation_error(err),
            "case {i}: expected validation error, got: {err}"
        );
    }
}

#[test]
fn non_validation_errors_are_not_classified_as_validation_errors() {
    let err = anyhow::anyhow!("something unexpected went wrong");
    assert!(
        !is_validation_error(&err),
        "plain anyhow errors must not be classified as validation errors"
    );
    assert!(
        action_validation_error(&err).is_none(),
        "plain anyhow errors must not expose action validation metadata"
    );
}

#[test]
fn wrapped_action_errors_still_classify_as_validation_errors() {
    let err: anyhow::Error = ActionError::from(ValidationError::MissingAction).into();
    let validation = action_validation_error(&err).expect("wrapped action error should classify");
    assert_eq!(validation.code(), "missing_action");
    assert!(is_validation_error(&err));
}

#[test]
fn scopes_satisfy_write_implies_read() {
    let write = vec![WRITE_SCOPE.to_string()];
    assert!(scopes_satisfy(&write, READ_SCOPE));
    assert!(scopes_satisfy(&write, WRITE_SCOPE));
}

#[test]
fn scopes_satisfy_read_does_not_imply_write() {
    let read = vec![READ_SCOPE.to_string()];
    assert!(!scopes_satisfy(&read, WRITE_SCOPE));
}
