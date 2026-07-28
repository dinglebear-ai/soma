use rmcp::model::{CacheScope, ErrorCode, ResourceContents};
use serde_json::json;
use soma_application::{ApplicationError, ResourceContent};
use soma_provider_core::ProviderResource;

use super::{
    private_dynamic_read_response, private_prompts_result, private_resource_templates_result,
    private_resources_result, private_tools_result, resource_contents_from_output,
    resource_read_error, rmcp_resource_from_catalog_resource, rmcp_tool_from_json,
};

#[test]
fn dynamic_catalog_results_are_private_and_immediately_stale() {
    let tools = private_tools_result(Vec::new());
    let resources = private_resources_result(Vec::new());
    let templates = private_resource_templates_result(Vec::new());
    let prompts = private_prompts_result(rmcp::model::ListPromptsResult::default());
    let resource = match private_dynamic_read_response(
        rmcp::model::ReadResourceResult::new(Vec::new()).into(),
    ) {
        rmcp::model::ReadResourceResponse::Complete(result) => result,
        rmcp::model::ReadResourceResponse::InputRequired(_) => {
            panic!("cache helper must preserve a complete resource response")
        }
        _ => panic!("unexpected future resource response variant"),
    };

    for (name, ttl_ms, cache_scope, wire) in [
        (
            "tools/list",
            tools.ttl_ms,
            tools.cache_scope,
            serde_json::to_value(&tools).expect("serialize tools/list"),
        ),
        (
            "resources/list",
            resources.ttl_ms,
            resources.cache_scope,
            serde_json::to_value(&resources).expect("serialize resources/list"),
        ),
        (
            "resources/templates/list",
            templates.ttl_ms,
            templates.cache_scope,
            serde_json::to_value(&templates).expect("serialize resource templates"),
        ),
        (
            "prompts/list",
            prompts.ttl_ms,
            prompts.cache_scope,
            serde_json::to_value(&prompts).expect("serialize prompts/list"),
        ),
        (
            "resources/read",
            resource.ttl_ms,
            resource.cache_scope,
            serde_json::to_value(&resource).expect("serialize resources/read"),
        ),
    ] {
        assert_eq!(ttl_ms, Some(0), "{name} must be immediately stale");
        assert_eq!(
            cache_scope,
            Some(CacheScope::Private),
            "{name} must remain user-private"
        );
        assert_eq!(wire["ttlMs"], 0, "{name} wire ttl");
        assert_eq!(wire["cacheScope"], "private", "{name} wire scope");
    }
}

#[test]
fn rmcp_tool_conversion_preserves_output_schema() {
    let tool = rmcp_tool_from_json(json!({
        "name": "soma",
        "description": "Dispatch Soma actions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": { "type": "string" }
            },
            "required": ["action"]
        },
        "outputSchema": {
            "type": "object",
            "additionalProperties": true,
            "properties": {
                "status": { "type": "string" }
            }
        }
    }))
    .expect("tool definition should convert");

    let schema = tool
        .output_schema
        .as_ref()
        .expect("outputSchema should be copied onto rmcp Tool");
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["status"]["type"], "string");
}

#[test]
fn resource_read_error_maps_unknown_resource_to_invalid_params() {
    let error = ApplicationError::new(
        "unknown_resource",
        "unknown resource",
        false,
        "List available resources and retry.",
    );
    let mapped = resource_read_error("soma://resources/missing", &error);
    assert_eq!(mapped.code, ErrorCode::INVALID_PARAMS);
    assert!(mapped.message.contains("unknown resource"));
}

#[test]
fn resource_read_error_maps_insufficient_scope_to_invalid_request() {
    let error = ApplicationError::new(
        "insufficient_scope",
        "resource `soma://resources/runbook` requires scope `soma:write`",
        false,
        "Authenticate with a token that includes the required scope.",
    );
    let mapped = resource_read_error("soma://resources/runbook", &error);
    assert_eq!(mapped.code, ErrorCode::INVALID_REQUEST);
    assert!(mapped.message.contains("forbidden"));
}

#[test]
fn resource_read_error_maps_every_other_code_to_internal_error() {
    for code in [
        "resource_reader_timeout",
        "resource_reader_invalid_shape",
        "resource_escapes_root",
        "provider_not_loaded",
    ] {
        let error = ApplicationError::new(code, "boom", false, "Retry later.");
        let mapped = resource_read_error("soma://resources/x", &error);
        assert_eq!(
            mapped.code,
            ErrorCode::INTERNAL_ERROR,
            "code {code} should map to internal_error"
        );
    }
}

#[test]
fn resource_contents_from_output_preserves_text_and_mime_type() {
    let contents = resource_contents_from_output(
        "soma://resources/runbook",
        ResourceContent::Text {
            text: "hello".to_owned(),
            mime_type: Some("text/markdown".to_owned()),
        },
    );
    match contents {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => {
            assert_eq!(uri, "soma://resources/runbook");
            assert_eq!(mime_type.as_deref(), Some("text/markdown"));
            assert_eq!(text, "hello");
        }
        ResourceContents::BlobResourceContents { .. } => panic!("expected text contents"),
        _ => panic!("unexpected resource contents variant"),
    }
}

#[test]
fn resource_contents_from_output_falls_back_to_text_plain_when_reader_omits_mime_type() {
    // `rmcp::model::ResourceContents::text` itself defaults to
    // `text/plain` when not overridden — `resource_contents_from_output`
    // only overrides it, it never clears it, so a reader that returns
    // `{ text }` with no `mimeType` still gets a real MIME type on the
    // wire rather than `null`.
    let contents = resource_contents_from_output(
        "soma://resources/runbook",
        ResourceContent::Text {
            text: "hello".to_owned(),
            mime_type: None,
        },
    );
    match contents {
        ResourceContents::TextResourceContents { mime_type, .. } => {
            assert_eq!(mime_type.as_deref(), Some("text/plain"));
        }
        ResourceContents::BlobResourceContents { .. } => panic!("expected text contents"),
        _ => panic!("unexpected resource contents variant"),
    }
}

#[test]
fn resource_contents_from_output_preserves_blob_and_mime_type() {
    let contents = resource_contents_from_output(
        "soma://resources/logo",
        ResourceContent::Blob {
            blob_base64: "AAAA".to_owned(),
            mime_type: Some("image/png".to_owned()),
        },
    );
    match contents {
        ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        } => {
            assert_eq!(uri, "soma://resources/logo");
            assert_eq!(mime_type.as_deref(), Some("image/png"));
            assert_eq!(blob, "AAAA");
        }
        ResourceContents::TextResourceContents { .. } => panic!("expected blob contents"),
        _ => panic!("unexpected resource contents variant"),
    }
}

#[test]
fn rmcp_resource_conversion_carries_uri_name_description_and_mime_type() {
    let resource = ProviderResource {
        uri_template: "soma://resources/runbook".to_owned(),
        name: "runbook".to_owned(),
        description: "On-call runbook".to_owned(),
        mime_type: Some("text/markdown".to_owned()),
        scope: None,
        mcp: None,
        annotations: json!({}),
    };
    let converted = rmcp_resource_from_catalog_resource(&resource);
    assert_eq!(converted.uri, "soma://resources/runbook");
    assert_eq!(converted.name, "runbook");
    assert_eq!(converted.description.as_deref(), Some("On-call runbook"));
    assert_eq!(converted.mime_type.as_deref(), Some("text/markdown"));
}
