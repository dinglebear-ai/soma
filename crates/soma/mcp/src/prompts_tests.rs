//! Unit tests for MCP-specific prompt conversion.

use super::*;
use soma_contracts::providers::ProviderPrompt;

#[test]
fn list_prompts_returns_quick_start() {
    let result = list_prompts();
    let names: Vec<&str> = result
        .prompts
        .iter()
        .map(|prompt| prompt.name.as_str())
        .collect();
    assert!(names.contains(&"quick_start"));
}

#[test]
fn get_prompt_quick_start_returns_message() {
    let result = get_prompt(rmcp::model::GetPromptRequestParams::new("quick_start"))
        .expect("quick_start should resolve");
    assert!(!result.messages.is_empty());
}

#[test]
fn get_prompt_unknown_returns_err() {
    let result = get_prompt(rmcp::model::GetPromptRequestParams::new("nonexistent"));
    assert!(result.is_err());
}

fn provider_prompt(name: &str, template: Option<&str>) -> ProviderPrompt {
    ProviderPrompt {
        name: name.to_owned(),
        title: None,
        description: "Review prompt".to_owned(),
        template: template.map(ToOwned::to_owned),
        arguments_schema: None,
        scope: None,
        mcp: None,
        examples: Vec::new(),
    }
}

#[test]
fn provider_prompts_are_listed_and_converted_to_results() {
    let prompt = provider_prompt("review", Some("# Review\n\nCheck correctness."));
    let listed = provider_prompts(std::slice::from_ref(&prompt));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "review");

    let value = serde_json::to_value(provider_prompt_result(prompt))
        .expect("prompt result should serialize");
    assert_eq!(
        value["messages"][0]["content"]["text"],
        "# Review\n\nCheck correctness."
    );
}

#[test]
fn provider_prompt_without_template_is_not_advertised() {
    let prompt = provider_prompt("reserved", None);
    assert!(provider_prompts(&[prompt]).is_empty());
}
