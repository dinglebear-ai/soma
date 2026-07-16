//! Official MCP conformance-suite reference fixtures.
//!
//! These fixtures are intentionally hidden behind
//! `SOMA_MCP_CONFORMANCE_FIXTURES=true`. They let Soma run
//! the upstream reference scenarios without advertising test-only tools,
//! resources, or prompts in real derived servers.

use std::{borrow::Cow, sync::Arc};

use rmcp::model::{
    AudioContent, CallToolResult, ContentBlock, GetPromptRequestParams, GetPromptResult,
    ImageContent, Prompt, PromptArgument, PromptMessage, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, Role, Tool,
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
        "test_simple_text" => CallToolResult::success(vec![ContentBlock::text(
            "This is a simple text response for testing.",
        )]),
        "test_image_content" => {
            CallToolResult::success(vec![ContentBlock::image(PNG_1X1_RED, "image/png")])
        }
        "test_audio_content" => CallToolResult::success(vec![ContentBlock::Audio(
            AudioContent::new(WAV_SILENCE, "audio/wav"),
        )]),
        "test_embedded_resource" => CallToolResult::success(vec![ContentBlock::resource(
            ResourceContents::text(
                "This is an embedded resource content.",
                "test://embedded-resource",
            )
            .with_mime_type("text/plain"),
        )]),
        "test_multiple_content_types" => CallToolResult::success(vec![
            ContentBlock::text("Multiple content types test:"),
            ContentBlock::image(PNG_1X1_RED, "image/png"),
            ContentBlock::resource(
                ResourceContents::text(
                    r#"{"test":"data","value":123}"#,
                    "test://mixed-content-resource",
                )
                .with_mime_type("application/json"),
            ),
        ]),
        "test_error_handling" => CallToolResult::error(vec![ContentBlock::text(
            "This tool intentionally returns an error for testing",
        )]),
        _ => return None,
    };
    Some(result)
}

pub(super) fn resources() -> Vec<Resource> {
    vec![
        Resource::new("test://static-text", "static text fixture")
            .with_description("MCP conformance text resource fixture")
            .with_mime_type("text/plain"),
        Resource::new("test://static-binary", "static binary fixture")
            .with_description("MCP conformance binary resource fixture")
            .with_mime_type("image/png"),
    ]
}

pub(super) fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new("test://template/{id}/data", "template data by id")
            .with_description("MCP conformance templated JSON resource fixture")
            .with_mime_type("application/json"),
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
            Role::User,
            "This is a simple prompt for testing.",
        )]),
        "test_prompt_with_arguments" => {
            let args = request.arguments.as_ref();
            let arg1 = prompt_arg(args, "arg1");
            let arg2 = prompt_arg(args, "arg2");
            GetPromptResult::new(vec![PromptMessage::new_text(
                Role::User,
                format!("Prompt with arguments: arg1='{arg1}', arg2='{arg2}'"),
            )])
        }
        "test_prompt_with_embedded_resource" => {
            let args = request.arguments.as_ref();
            let uri = prompt_arg(args, "resourceUri");
            GetPromptResult::new(vec![
                PromptMessage::new_resource(
                    Role::User,
                    uri,
                    Some("text/plain".to_string()),
                    Some("Embedded resource content for testing.".to_string()),
                    None,
                    None,
                    None,
                ),
                PromptMessage::new_text(Role::User, "Please process the embedded resource above."),
            ])
        }
        "test_prompt_with_image" => GetPromptResult::new(vec![
            PromptMessage::new(
                Role::User,
                ContentBlock::Image(ImageContent::new(PNG_1X1_RED, "image/png")),
            ),
            PromptMessage::new_text(Role::User, "Please analyze the image above."),
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
#[path = "conformance_tests.rs"]
mod tests;
