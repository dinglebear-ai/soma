use crate::host::StepDecision;

pub fn step_error(message: impl Into<String>) -> StepDecision {
    StepDecision::Error {
        kind: "step_error".to_string(),
        message: message.into(),
    }
}
