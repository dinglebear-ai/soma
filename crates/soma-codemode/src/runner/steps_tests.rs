use serde_json::json;

use super::steps::replay_or_execute;
use crate::host::StepDecision;

#[test]
fn step_decision_replays_when_value_exists() {
    assert_eq!(
        replay_or_execute(Some(json!(1))),
        StepDecision::Replay(json!(1))
    );
    assert_eq!(replay_or_execute(None), StepDecision::Execute);
}
