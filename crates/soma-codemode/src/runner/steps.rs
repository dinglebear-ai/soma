use serde_json::Value;

use crate::host::StepDecision;

pub fn replay_or_execute(value: Option<Value>) -> StepDecision {
    value.map_or(StepDecision::Execute, StepDecision::Replay)
}
