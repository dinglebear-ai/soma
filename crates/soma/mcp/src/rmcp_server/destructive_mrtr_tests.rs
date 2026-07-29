use std::collections::BTreeMap;

use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, ElicitationCapability, FormElicitationCapability,
    Implementation, ProtocolVersion, RequestMetaObject,
};
use serde_json::json;
use soma_domain::Confirmation;

use super::{DESTRUCTIVE_CONFIRMATION_INPUT, DestructiveConfirmation, destructive_confirmation};

fn capable_meta() -> RequestMetaObject {
    let capabilities = ClientCapabilities::builder()
        .enable_elicitation_with(
            ElicitationCapability::new().with_form(FormElicitationCapability::new()),
        )
        .build();
    RequestMetaObject::with_client_context(
        ProtocolVersion::V_2026_07_28,
        Implementation::new("test-client", "1.0.0"),
        capabilities,
    )
}

#[test]
fn non_destructive_action_does_not_change_confirmation_context() {
    assert!(matches!(
        destructive_confirmation(
            &CallToolRequestParams::new("soma"),
            &RequestMetaObject::default(),
            "status",
            false,
        ),
        DestructiveConfirmation::Proceed(Confirmation::Missing)
    ));
}

#[test]
fn legacy_confirm_argument_remains_supported() {
    let mut request = CallToolRequestParams::new("soma");
    request.arguments = Some(serde_json::Map::from_iter([(
        "confirm".to_owned(),
        json!(true),
    )]));

    assert!(matches!(
        destructive_confirmation(&request, &RequestMetaObject::default(), "delete", true),
        DestructiveConfirmation::Proceed(Confirmation::Confirmed)
    ));
}

#[test]
fn capable_client_receives_keyed_form_elicitation_without_request_state() {
    let request = CallToolRequestParams::new("soma");
    let DestructiveConfirmation::InputRequired(result) =
        destructive_confirmation(&request, &capable_meta(), "delete", true)
    else {
        panic!("expected input_required");
    };

    assert!(result.request_state.is_none());
    let requests = result.input_requests.clone().expect("inputRequests");
    assert_eq!(requests.len(), 1);
    assert!(requests.contains_key(DESTRUCTIVE_CONFIRMATION_INPUT));
    let wire = serde_json::to_value(result).expect("serialize input_required");
    assert_eq!(wire["resultType"], "input_required");
    assert_eq!(
        wire["inputRequests"][DESTRUCTIVE_CONFIRMATION_INPUT]["method"],
        "elicitation/create"
    );
    assert_eq!(
        wire["inputRequests"][DESTRUCTIVE_CONFIRMATION_INPUT]["params"]["mode"],
        "form"
    );
}

#[test]
fn accepted_retry_confirms_the_existing_domain_gate() {
    let mut request = CallToolRequestParams::new("soma");
    request.input_responses = Some(BTreeMap::from([(
        DESTRUCTIVE_CONFIRMATION_INPUT.to_owned(),
        json!({"action": "accept", "content": {"confirm": true}}),
    )]));

    assert!(matches!(
        destructive_confirmation(&request, &RequestMetaObject::default(), "delete", true),
        DestructiveConfirmation::Proceed(Confirmation::Confirmed)
    ));
}

#[test]
fn malformed_declined_or_unsupported_confirmation_fails_closed() {
    for response in [
        json!({"action": "decline"}),
        json!({"action": "cancel"}),
        json!({"action": "accept", "content": {"confirm": false}}),
        json!({"action": "accept", "content": {}}),
        json!({"unexpected": true}),
    ] {
        let mut request = CallToolRequestParams::new("soma");
        request.input_responses = Some(BTreeMap::from([(
            DESTRUCTIVE_CONFIRMATION_INPUT.to_owned(),
            response,
        )]));
        assert!(matches!(
            destructive_confirmation(&request, &RequestMetaObject::default(), "delete", true),
            DestructiveConfirmation::Refused
        ));
    }

    assert!(matches!(
        destructive_confirmation(
            &CallToolRequestParams::new("soma"),
            &RequestMetaObject::default(),
            "delete",
            true,
        ),
        DestructiveConfirmation::Refused
    ));
}
