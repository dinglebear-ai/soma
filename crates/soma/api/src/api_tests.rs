use serde_json::json;

use super::{optional_name_params, rest_params, split_rest_transport_metadata};
use soma_domain::{Confirmation, actions::SomaAction};

#[test]
fn rest_dto_translation_stays_in_the_http_adapter() {
    assert_eq!(optional_name_params(None), json!({}));
    assert_eq!(
        rest_params(&SomaAction::Echo {
            message: "hello".to_owned(),
        }),
        json!({"message": "hello"})
    );
}

#[test]
fn rest_confirmation_requires_an_explicit_true_boolean() {
    let (params, confirmation) = split_rest_transport_metadata(
        "python_environment_prune",
        json!({"confirm": true, "value": 7}),
    );
    assert_eq!(params, json!({"value": 7}));
    assert_eq!(confirmation, Confirmation::Confirmed);

    let (params, confirmation) =
        split_rest_transport_metadata("python_environment_prune", json!({"confirm": false}));
    assert_eq!(params, json!({}));
    assert_eq!(
        confirmation,
        Confirmation::Missing,
        "false is stripped but does not confirm"
    );

    let (params, confirmation) =
        split_rest_transport_metadata("python_environment_prune", json!({"confirm": "true"}));
    assert_eq!(params, json!({}));
    assert_eq!(confirmation, Confirmation::Missing);
}

#[test]
fn rest_dto_preserves_every_python_operator_parameter() {
    assert_eq!(
        rest_params(&SomaAction::PythonEnvironmentPrunePlan {
            stale_before_unix_seconds: 123,
            max_entries: 9,
        }),
        json!({"stale_before_unix_seconds": 123, "max_entries": 9})
    );
    assert_eq!(
        rest_params(&SomaAction::PythonEnvironmentRepair {
            provider_path: "providers/example.py".to_owned(),
        }),
        json!({"provider_path": "providers/example.py"})
    );
    assert_eq!(
        rest_params(&SomaAction::PythonWorkerCancel {
            provider: "example".to_owned(),
        }),
        json!({"provider": "example"})
    );
    assert_eq!(
        rest_params(&SomaAction::PythonGenerationRollback { generation_id: 7 }),
        json!({"generation_id": 7})
    );
}

#[test]
fn dynamic_provider_confirm_parameter_is_not_transport_metadata() {
    let payload = json!({"confirm": true, "value": 7});
    let (params, confirmation) =
        split_rest_transport_metadata("dynamic_provider_action", payload.clone());
    assert_eq!(params, payload);
    assert_eq!(confirmation, Confirmation::Missing);
}
