//! Official MCP conformance-suite reference fixtures.
//!
//! These fixtures are intentionally hidden behind
//! `RTEMPLATE_MCP_CONFORMANCE_FIXTURES=true`. They let the generic template run
//! the upstream reference scenarios without advertising test-only tools,
//! resources, or prompts in real derived servers.

use std::{borrow::Cow, sync::Arc};

use rmcp::model::{
    AnnotateAble, CallToolResult, Content, GetPromptRequestParams, GetPromptResult, Prompt,
    PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole, RawAudioContent,
    RawContent, RawImageContent, RawResource, ReadResourceResult, Resource, ResourceContents, Tool,
};
use serde_json::{json, Map, Value};

const PNG_1X1_RED: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR42mP8z8BQDwAFgwJ/lZc47wAAAABJRU5ErkJggg==";
const WAV_SILENCE: &str = "UklGRiQAAABXQVZFZm10IBAAAAABAAEAESsAACJWAAACABAAZGF0YQAAAAA=";

pub(super) fn tool_definitions() -> Vec<Tool> {
    [
        ("test_simple_text", "Returns a simple text content block."),
        (
            "test_image_content",
            "Returns a minimal PNG image content block.",
        ),
        (
            "test_audio_content",
            "Returns a minimal WAV audio content block.",
        ),
        (
            "test_embedded_resource",
            "Returns an embedded text resource content block.",
        ),
        (
            "test_multiple_content_types",
            "Returns text, image, and embedded resource content together.",
        ),
        (
            "test_error_handling",
            "Returns an MCP tool result with isError=true.",
        ),
        (
            "json_schema_2020_12_tool",
            "Tool with JSON Schema 2020-12 features.",
        ),
    ]
    .into_iter()
    .map(|(name, description)| {
        let schema = if name == "json_schema_2020_12_tool" {
            json_schema_2020_12_input_schema()
        } else {
            empty_input_schema()
        };
        Tool::new_with_raw(
            Cow::Owned(name.to_string()),
            Some(Cow::Owned(description.to_string())),
            Arc::new(schema),
        )
    })
    .collect()
}

pub(super) fn call_tool(name: &str) -> Option<CallToolResult> {
    let result = match name {
        "test_simple_text" => CallToolResult::success(vec![Content::text(
            "This is a simple text response for testing.",
        )]),
        "test_image_content" => {
            CallToolResult::success(vec![Content::image(PNG_1X1_RED, "image/png")])
        }
        "test_audio_content" => CallToolResult::success(vec![RawContent::Audio(RawAudioContent {
            data: WAV_SILENCE.to_string(),
            mime_type: "audio/wav".to_string(),
        })
        .no_annotation()]),
        "test_embedded_resource" => CallToolResult::success(vec![Content::resource(
            ResourceContents::text(
                "This is an embedded resource content.",
                "test://embedded-resource",
            )
            .with_mime_type("text/plain"),
        )]),
        "test_multiple_content_types" => CallToolResult::success(vec![
            Content::text("Multiple content types test:"),
            Content::image(PNG_1X1_RED, "image/png"),
            Content::resource(
                ResourceContents::text(
                    r#"{"test":"data","value":123}"#,
                    "test://mixed-content-resource",
                )
                .with_mime_type("application/json"),
            ),
        ]),
        "test_error_handling" => CallToolResult::error(vec![Content::text(
            "This tool intentionally returns an error for testing",
        )]),
        _ => return None,
    };
    Some(result)
}

pub(super) fn resources() -> Vec<Resource> {
    vec![
        Resource::new(
            RawResource::new("test://static-text", "static text fixture")
                .with_description("MCP conformance text resource fixture")
                .with_mime_type("text/plain"),
            None,
        ),
        Resource::new(
            RawResource::new("test://static-binary", "static binary fixture")
                .with_description("MCP conformance binary resource fixture")
                .with_mime_type("image/png"),
            None,
        ),
    ]
}

pub(super) fn read_resource(uri: &str) -> Option<ReadResourceResult> {
    let contents = match uri {
        "test://static-text" => vec![ResourceContents::text(
            "This is the content of the static text resource.",
            "test://static-text",
        )
        .with_mime_type("text/plain")],
        "test://static-binary" => {
            vec![ResourceContents::blob(PNG_1X1_RED, "test://static-binary")
                .with_mime_type("image/png")]
        }
        "test://template/123/data" => vec![ResourceContents::text(
            r#"{"id":"123","templateTest":true,"data":"Data for ID: 123"}"#,
            "test://template/123/data",
        )
        .with_mime_type("application/json")],
        _ => return None,
    };
    Some(ReadResourceResult::new(contents))
}

pub(super) fn prompts() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "test_simple_prompt",
            Some("MCP conformance simple prompt fixture"),
            None,
        ),
        Prompt::new(
            "test_prompt_with_arguments",
            Some("MCP conformance argument-substitution prompt fixture"),
            Some(vec![
                PromptArgument::new("arg1")
                    .with_description("First test argument")
                    .with_required(true),
                PromptArgument::new("arg2")
                    .with_description("Second test argument")
                    .with_required(true),
            ]),
        ),
        Prompt::new(
            "test_prompt_with_embedded_resource",
            Some("MCP conformance embedded-resource prompt fixture"),
            Some(vec![PromptArgument::new("resourceUri")
                .with_description("URI of the resource to embed")
                .with_required(true)]),
        ),
        Prompt::new(
            "test_prompt_with_image",
            Some("MCP conformance image prompt fixture"),
            None,
        ),
    ]
}

pub(super) fn get_prompt(request: GetPromptRequestParams) -> Option<GetPromptResult> {
    let result = match request.name.as_str() {
        "test_simple_prompt" => GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            "This is a simple prompt for testing.",
        )]),
        "test_prompt_with_arguments" => {
            let args = request.arguments.as_ref();
            let arg1 = prompt_arg(args, "arg1");
            let arg2 = prompt_arg(args, "arg2");
            GetPromptResult::new(vec![PromptMessage::new_text(
                PromptMessageRole::User,
                format!("Prompt with arguments: arg1='{arg1}', arg2='{arg2}'"),
            )])
        }
        "test_prompt_with_embedded_resource" => {
            let args = request.arguments.as_ref();
            let uri = prompt_arg(args, "resourceUri");
            GetPromptResult::new(vec![
                PromptMessage::new_resource(
                    PromptMessageRole::User,
                    uri,
                    Some("text/plain".to_string()),
                    Some("Embedded resource content for testing.".to_string()),
                    None,
                    None,
                    None,
                ),
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    "Please process the embedded resource above.",
                ),
            ])
        }
        "test_prompt_with_image" => GetPromptResult::new(vec![
            PromptMessage::new(
                PromptMessageRole::User,
                PromptMessageContent::Image {
                    image: RawImageContent {
                        data: PNG_1X1_RED.to_string(),
                        mime_type: "image/png".to_string(),
                        meta: None,
                    }
                    .no_annotation(),
                },
            ),
            PromptMessage::new_text(PromptMessageRole::User, "Please analyze the image above."),
        ]),
        _ => return None,
    };
    Some(result)
}

fn empty_input_schema() -> Map<String, Value> {
    serde_json::from_value(json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    }))
    .expect("static conformance input schema must be a JSON object")
}

fn json_schema_2020_12_input_schema() -> Map<String, Value> {
    serde_json::from_value(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "$defs": {
            "address": {
                "$anchor": "addressDef",
                "type": "object",
                "properties": {
                    "street": { "type": "string" },
                    "city": { "type": "string" }
                }
            }
        },
        "properties": {
            "name": { "type": "string" },
            "address": { "$ref": "#/$defs/address" },
            "contactMethod": { "type": "string", "enum": ["phone", "email"] },
            "phone": { "type": "string" },
            "email": { "type": "string" }
        },
        "allOf": [
            { "anyOf": [{ "required": ["phone"] }, { "required": ["email"] }] }
        ],
        "if": {
            "properties": { "contactMethod": { "const": "phone" } },
            "required": ["contactMethod"]
        },
        "then": { "required": ["phone"] },
        "else": { "required": ["email"] },
        "additionalProperties": false
    }))
    .expect("static JSON Schema 2020-12 fixture must be a JSON object")
}

fn prompt_arg(args: Option<&Map<String, Value>>, name: &str) -> String {
    args.and_then(|m| m.get(name))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
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
        assert!(value["contents"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("123"));
    }

    #[test]
    fn image_prompt_fixture_serializes_image_message() {
        let result = get_prompt(GetPromptRequestParams::new("test_prompt_with_image"))
            .expect("fixture should exist");
        let value = serde_json::to_value(result).expect("prompt should serialize");

        assert_eq!(value["messages"][0]["content"]["type"], "image");
        assert_eq!(value["messages"][0]["content"]["mimeType"], "image/png");
    }
}
