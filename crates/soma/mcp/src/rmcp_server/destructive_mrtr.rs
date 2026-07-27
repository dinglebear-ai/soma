use rmcp::model::{
    BooleanSchema, CallToolRequestParams, ElicitRequest, ElicitRequestParams, ElicitResult,
    ElicitationAction, ElicitationSchema, InputRequest, InputRequests, InputRequiredResult,
    PrimitiveSchemaDefinition, RequestMetaObject,
};
use serde_json::Value;
use soma_domain::Confirmation;

pub(super) const DESTRUCTIVE_CONFIRMATION_INPUT: &str = "destructive_confirmation";

pub(super) enum DestructiveConfirmation {
    Proceed(Confirmation),
    InputRequired(InputRequiredResult),
    Refused,
}

pub(super) fn destructive_confirmation(
    request: &CallToolRequestParams,
    meta: &RequestMetaObject,
    action: &str,
    requires_confirmation: bool,
) -> DestructiveConfirmation {
    if !requires_confirmation {
        return DestructiveConfirmation::Proceed(Confirmation::Missing);
    }

    if request
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get("confirm"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return DestructiveConfirmation::Proceed(Confirmation::Confirmed);
    }

    if let Some(responses) = request.input_responses.as_ref() {
        let accepted = responses
            .get(DESTRUCTIVE_CONFIRMATION_INPUT)
            .and_then(|value| serde_json::from_value::<ElicitResult>(value.clone()).ok())
            .is_some_and(|result| {
                result.action == ElicitationAction::Accept
                    && result
                        .content
                        .as_ref()
                        .and_then(|content| content.get("confirm"))
                        .and_then(Value::as_bool)
                        == Some(true)
            });
        return if accepted {
            DestructiveConfirmation::Proceed(Confirmation::Confirmed)
        } else {
            DestructiveConfirmation::Refused
        };
    }

    let supports_form_elicitation = meta
        .client_capabilities()
        .and_then(|capabilities| capabilities.elicitation)
        .and_then(|elicitation| elicitation.form)
        .is_some();
    if !supports_form_elicitation {
        return DestructiveConfirmation::Refused;
    }

    let Ok(schema) = ElicitationSchema::builder()
        .required_property(
            "confirm",
            PrimitiveSchemaDefinition::Boolean(BooleanSchema::default()),
        )
        .build()
    else {
        return DestructiveConfirmation::Refused;
    };
    let params = ElicitRequestParams::FormElicitationParams {
        meta: None,
        message: format!(
            "Action soma.{action} is destructive and may not be reversible. Set confirm to true to proceed."
        ),
        requested_schema: schema,
    };
    let input_requests = InputRequests::from([(
        DESTRUCTIVE_CONFIRMATION_INPUT.to_owned(),
        InputRequest::Elicitation(ElicitRequest::new(params)),
    )]);
    DestructiveConfirmation::InputRequired(InputRequiredResult::from_input_requests(input_requests))
}

#[cfg(test)]
#[path = "destructive_mrtr_tests.rs"]
mod tests;
