use rmcp::model::GetPromptRequestParams;
use serde_json::Value;

use super::*;

#[test]
fn fixture_tools_include_reference_names_without_deprecated_tools() {
    let names: Vec<_> = tool_definitions()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();

    assert!(names.contains(&"test_simple_text".to_string()));
    assert!(names.contains(&"test_image_content".to_string()));
    assert!(names.contains(&"test_audio_content".to_string()));
    assert!(names.contains(&"test_embedded_resource".to_string()));
    assert!(names.contains(&"test_multiple_content_types".to_string()));
    assert!(names.contains(&"test_error_handling".to_string()));
    assert!(names.contains(&"json_schema_2020_12_tool".to_string()));
    assert!(!names.contains(&"test_tool_with_logging".to_string()));
    assert!(!names.contains(&"test_sampling".to_string()));
}

#[test]
fn json_schema_2020_12_fixture_preserves_extended_keywords() {
    let tool = tool_definitions()
        .into_iter()
        .find(|tool| tool.name == "json_schema_2020_12_tool")
        .expect("fixture should exist");
    let schema = tool.input_schema.as_ref();

    assert_eq!(
        schema.get("$schema").and_then(Value::as_str),
        Some("https://json-schema.org/draft/2020-12/schema")
    );
    assert!(schema.contains_key("$defs"));
    assert!(schema.contains_key("allOf"));
    assert!(schema.contains_key("if"));
    assert!(schema.contains_key("then"));
    assert!(schema.contains_key("else"));
    assert_eq!(
        schema.get("additionalProperties"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn mixed_content_fixture_serializes_text_image_and_resource() {
    let result = call_tool("test_multiple_content_types").expect("fixture should exist");
    let value = serde_json::to_value(result).expect("fixture should serialize");
    let content = value["content"]
        .as_array()
        .expect("content should be an array");

    assert!(content.iter().any(|item| item["type"] == "text"));
    assert!(content.iter().any(|item| item["type"] == "image"));
    assert!(content.iter().any(|item| item["type"] == "resource"));
}

#[test]
fn resource_template_fixture_reflects_substituted_id() {
    let result = read_resource("test://template/123/data").expect("fixture should exist");
    let value = serde_json::to_value(result).expect("resource should serialize");

    assert_eq!(value["contents"][0]["uri"], "test://template/123/data");
    let text = value["contents"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains(r#""id":"123""#));
    assert!(text.contains(r#""templateTest":true"#));
}

#[test]
fn resource_templates_advertise_substitutable_fixture_uri() {
    let templates = resource_templates();
    let value = serde_json::to_value(&templates[0]).expect("template should serialize");

    assert_eq!(value["uriTemplate"], "test://template/{id}/data");
    assert_eq!(value["name"], "template data by id");
    assert_eq!(value["mimeType"], "application/json");
}

#[test]
fn image_prompt_fixture_serializes_image_message() {
    let result = get_prompt(GetPromptRequestParams::new("test_prompt_with_image"))
        .expect("fixture should exist");
    let value = serde_json::to_value(result).expect("prompt should serialize");

    assert_eq!(value["messages"][0]["content"]["type"], "image");
    assert_eq!(value["messages"][0]["content"]["mimeType"], "image/png");
}
