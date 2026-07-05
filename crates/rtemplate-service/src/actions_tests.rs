use serde_json::json;

use crate::actions::{action_registry, action_specs, execute_native_action, validate_params};
use crate::{ExampleClient, ExampleService};
use rtemplate_contracts::actions::{ActionTransport, READ_SCOPE};

#[test]
fn registry_has_single_source_action_metadata() {
    let names: Vec<_> = action_specs().iter().map(|spec| spec.name).collect();
    assert_eq!(
        names,
        vec![
            "greet",
            "echo",
            "status",
            "help",
            "elicit_name",
            "scaffold_intent"
        ]
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

#[tokio::test]
async fn test_only_reverse_proves_action_execution_without_surface_code() {
    let cfg = rtemplate_contracts::config::ExampleConfig::default();
    let service = ExampleService::new(ExampleClient::new(&cfg).unwrap());
    let value = crate::actions::execute_test_reverse(&service, &json!({"text": "stressed"}))
        .await
        .unwrap();
    assert_eq!(value["reversed"], "desserts");
}
