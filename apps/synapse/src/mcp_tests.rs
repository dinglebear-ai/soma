use super::*;
use crate::SynapseConfig;

#[test]
fn mcp_exposes_canonical_and_optional_legacy_tools() {
    let runtime = StandaloneRuntime::from_config(SynapseConfig::default()).unwrap();
    let tools = tool_definitions(&runtime);
    assert_eq!(tools.len(), 3);
    assert_eq!(tools[0].name.as_ref(), "synapse");
    assert_eq!(tools[1].name.as_ref(), "flux");
    assert_eq!(tools[2].name.as_ref(), "scout");
}

#[test]
fn canonical_schema_covers_every_operation() {
    let runtime = StandaloneRuntime::from_config(SynapseConfig::default()).unwrap();
    let schema = canonical_schema(&runtime);
    let operations = schema["properties"]["operation"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(operations.len(), 59);
}

#[test]
fn mutation_elicitation_requires_two_explicit_affirmations() {
    let schema = schemars::schema_for!(ConfirmMutation);
    let value = serde_json::to_value(schema).unwrap();
    let required = value["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("confirm")));
    assert!(required.contains(&serde_json::json!("understood")));
}
