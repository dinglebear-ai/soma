use rmcp::{
    model::{CallToolRequestParams, ErrorCode, Meta, ResourceContents},
    service::ServiceError,
    ServiceExt,
};
use rmcp_traces::TraceTrust;
use serde_json::json;
use soma_test_support::{tracing_test_lock, SharedBuf};

use soma_application::{ApplicationError, ApplicationErrorDetails, ResourceContent};
use soma_domain::token_limit::MAX_RESPONSE_BYTES;
use soma_provider_core::ProviderResource;

use crate::assert_result_has_no_meta;

use super::{
    application_error_payload, resource_contents_from_output, resource_read_error,
    rmcp_resource_from_catalog_resource, rmcp_tool_from_json, tool_error_result,
    trace_context_from_meta, trace_summary_from_meta, unknown_tool_error,
};

const VALID_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";

#[test]
fn valid_mcp_trace_metadata_becomes_application_trace_context() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");

    let trace = trace_context_from_meta(&meta).expect("valid trace context");
    assert_eq!(trace.traceparent.as_deref(), Some(VALID_TRACEPARENT));
    assert_eq!(trace.tracestate.as_deref(), Some("vendor=value"));

    meta.set_traceparent("not-a-traceparent");
    assert!(trace_context_from_meta(&meta).is_none());
}

#[test]
fn application_errors_become_structured_tool_errors() {
    let error = anyhow::Error::new(ApplicationError::new(
        "missing_field",
        "missing required field `message`",
        false,
        "Provide `message` and retry.",
    ));
    let payload = application_error_payload(&error, "soma", Some("echo"));
    let result = tool_error_result(payload).expect("tool error should serialize");

    assert_result_has_no_meta(&result);
    assert_eq!(result.is_error, Some(true));
    let structured = result
        .structured_content
        .as_ref()
        .expect("structured content should be present");
    assert_eq!(structured["kind"], "mcp_tool_error");
    assert_eq!(structured["schema_version"], 1);
    assert_eq!(structured["code"], "missing_field");
    assert_eq!(structured["tool"], "soma");
    assert_eq!(structured["action"], "echo");
    assert!(structured["remediation"]
        .as_str()
        .unwrap_or_default()
        .contains("message"));
}

#[test]
fn provider_application_errors_preserve_mcp_taxonomy() {
    let error = anyhow::Error::new(
        ApplicationError::new(
            "provider_execution_failed",
            "Provider execution failed. Check server logs for details.",
            true,
            "Check provider status and retry.",
        )
        .with_details(ApplicationErrorDetails::Provider {
            schema_version: 1,
            provider: "weather".to_owned(),
            action: Some("weather_current".to_owned()),
            provider_error_kind: "provider_error".to_owned(),
        }),
    );

    let payload = application_error_payload(&error, "soma", Some("fallback"));

    assert_eq!(payload["provider"], "weather");
    assert_eq!(payload["action"], "weather_current");
    assert_eq!(payload["provider_error_kind"], "provider_error");
    assert_eq!(payload["retryable"], true);
}

#[test]
fn service_application_errors_preserve_validation_context() {
    let error = anyhow::Error::new(
        ApplicationError::new(
            "invalid_crate_name",
            "invalid crate name",
            true,
            "Use a Cargo-compatible crate name.",
        )
        .with_details(ApplicationErrorDetails::Service {
            schema_version: 1,
            service_error_kind: "validation".to_owned(),
            field: Some("crate_name".to_owned()),
            bad_value: None,
            expected_pattern: Some("^[a-z][a-z0-9-]*$".to_owned()),
            reason_kind: None,
            available_actions: Vec::new(),
        }),
    );

    let payload = application_error_payload(&error, "soma", Some("scaffold_intent"));

    assert_eq!(payload["service_error_kind"], "validation");
    assert_eq!(payload["field"], "crate_name");
    assert_eq!(payload["expected_pattern"], "^[a-z][a-z0-9-]*$");
}

#[test]
fn oversized_tool_errors_return_valid_overflow_envelope() {
    let result = tool_error_result(json!({
        "kind": "mcp_tool_error",
        "schema_version": 1,
        "code": "huge_error",
        "message": "x".repeat(MAX_RESPONSE_BYTES + 1),
    }))
    .expect("tool error should serialize");
    let text = result.content[0]
        .as_text()
        .expect("tool error should contain text")
        .text
        .as_str();
    let parsed: serde_json::Value =
        serde_json::from_str(text).expect("overflow error text should remain valid JSON");

    assert_result_has_no_meta(&result);
    assert_eq!(result.is_error, Some(true));
    assert_eq!(parsed["kind"], "mcp_tool_error");
    assert_eq!(parsed["code"], "error_payload_too_large");
    assert_eq!(parsed["original_code"], "huge_error");
    assert!(parsed["serialized_bytes"].as_u64().unwrap() > MAX_RESPONSE_BYTES as u64);
    assert_eq!(result.structured_content.as_ref(), Some(&parsed));
}

#[test]
fn unknown_tool_stays_protocol_error_with_structured_data() {
    let error = unknown_tool_error("bad_tool");

    assert!(error.message.contains("unknown tool"));
    let data = error
        .data
        .expect("unknown tool should include structured data");
    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "unknown_tool");
    assert_eq!(data["tool"], "bad_tool");
    assert_eq!(data["available_tools"], json!(["soma"]));
}

#[test]
fn execution_errors_do_not_expose_raw_error_text() {
    let raw_error = anyhow::anyhow!("upstream timeout talking to secret-api-key");
    let payload = application_error_payload(&raw_error, "soma", Some("status"));

    assert_eq!(
        payload,
        json!({
            "kind": "mcp_tool_error",
            "schema_version": 1,
            "code": "execution_error",
            "tool": "soma",
            "action": "status",
            "message": "Tool execution failed. Check server logs for details.",
            "retryable": true,
            "remediation": "Check service configuration and upstream availability, then retry. Use action=status or action=help for diagnostics.",
            "service_error_kind": "timeout",
            "reason_kind": "timeout",
        })
    );
    assert!(!payload.to_string().contains("secret-api-key"));
}

#[test]
fn trace_summary_for_logs_uses_untrusted_fail_soft_policy() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");
    meta.set_baggage("a".repeat(9 * 1024));

    let summary = trace_summary_from_meta(&meta);

    assert_eq!(summary.trace_id_prefix(), Some("0af76519"));
    assert_eq!(summary.span_id_prefix(), Some("00f067aa"));
    assert_eq!(summary.sampled(), Some(true));
    assert_eq!(summary.trust(), TraceTrust::Untrusted);
    assert!(summary.has_tracestate());
    assert_eq!(summary.invalid_count(), 1);
    assert_eq!(
        summary.invalid_reasons()[0],
        "baggage exceeded 8192 bytes (actual 9216)"
    );
    assert_eq!(summary.baggage_member_count(), 0);
    assert_eq!(summary.sensitive_baggage_member_count(), 0);
}

#[test]
fn valid_traceparent_survives_invalid_optional_trace_metadata() {
    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("invalid tracestate");
    meta.set_baggage("a".repeat(9 * 1024));

    let trace = trace_context_from_meta(&meta).expect("valid traceparent should propagate");

    assert_eq!(trace.traceparent.as_deref(), Some(VALID_TRACEPARENT));
    assert_eq!(trace.tracestate, None);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn call_tool_logs_safe_trace_summary_from_request_meta() {
    let _lock = tracing_test_lock();
    let buf = SharedBuf::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.writer())
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);

    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
    let server = super::rmcp_server(crate::testing::loopback_state());
    let server_handle = tokio::spawn(async move {
        let running = server
            .serve(server_transport)
            .await
            .expect("server should handshake");
        running.waiting().await.expect("server should stop cleanly");
    });
    let mut client = ().serve(client_transport).await.expect("client should handshake");

    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");
    meta.set_baggage(
        "email=alice@example.com,accessToken=super-secret-token,x-api-key=abc123,sessionId=s123",
    );
    let mut request = CallToolRequestParams::new("soma").with_arguments(
        serde_json::Map::from_iter([("action".to_owned(), json!("status"))]),
    );
    request.meta = Some(meta);

    let result = client
        .call_tool(request)
        .await
        .expect("status call should succeed");
    assert_result_has_no_meta(&result);

    client.close().await.expect("client should close");
    server_handle.await.expect("server task should join");
    drop(guard);

    let logs = buf.contents();
    assert!(
        logs.contains("MCP tool execution started"),
        "logs were: {logs}"
    );
    assert!(
        logs.contains("MCP tool execution completed"),
        "logs were: {logs}"
    );
    assert!(logs.contains("trace_id_prefix"), "logs were: {logs}");
    assert!(logs.contains("0af76519"), "logs were: {logs}");
    assert!(logs.contains("span_id_prefix"), "logs were: {logs}");
    assert!(logs.contains("00f067aa"), "logs were: {logs}");
    assert!(logs.contains("baggage_member_count=4"), "logs were: {logs}");
    assert!(
        logs.contains("sensitive_baggage_member_count=3"),
        "logs were: {logs}"
    );
    assert!(!logs.contains(VALID_TRACEPARENT), "logs were: {logs}");
    assert!(!logs.contains("vendor=value"), "logs were: {logs}");
    assert!(!logs.contains("alice@example.com"), "logs were: {logs}");
    assert!(!logs.contains("super-secret-token"), "logs were: {logs}");
    assert!(!logs.contains("abc123"), "logs were: {logs}");
    assert!(!logs.contains("s123"), "logs were: {logs}");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn call_tool_auth_failure_logs_without_trace_fields() {
    let _lock = tracing_test_lock();
    let buf = SharedBuf::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.writer())
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);

    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
    let server = super::rmcp_server(crate::testing::bearer_state("secret"));
    let server_handle = tokio::spawn(async move {
        let running = server
            .serve(server_transport)
            .await
            .expect("server should handshake");
        running.waiting().await.expect("server should stop cleanly");
    });
    let mut client = ().serve(client_transport).await.expect("client should handshake");

    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");
    meta.set_baggage("email=alice@example.com,sessionId=s123");
    let mut request = CallToolRequestParams::new("soma").with_arguments(
        serde_json::Map::from_iter([("action".to_owned(), json!("status"))]),
    );
    request.meta = Some(meta);

    let error = client
        .call_tool(request)
        .await
        .expect_err("mounted auth should reject missing HTTP auth context");
    let ServiceError::McpError(error) = error else {
        panic!("expected MCP protocol error, got: {error}");
    };
    assert!(error.message.contains("missing http context"));

    client.close().await.expect("client should close");
    server_handle.await.expect("server task should join");
    drop(guard);

    let logs = buf.contents();
    assert!(
        logs.contains("MCP tool rejected auth context"),
        "logs were: {logs}"
    );
    assert!(!logs.contains("trace_id_prefix"), "logs were: {logs}");
    assert!(!logs.contains("span_id_prefix"), "logs were: {logs}");
    assert!(!logs.contains("trace_invalid"), "logs were: {logs}");
    assert!(!logs.contains(VALID_TRACEPARENT), "logs were: {logs}");
    assert!(!logs.contains("vendor=value"), "logs were: {logs}");
    assert!(!logs.contains("alice@example.com"), "logs were: {logs}");
    assert!(!logs.contains("s123"), "logs were: {logs}");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn call_tool_response_page_rejection_logs_without_trace_or_request_fields() {
    let _lock = tracing_test_lock();
    let buf = SharedBuf::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.writer())
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);

    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
    let server = super::rmcp_server(crate::testing::loopback_state());
    let server_handle = tokio::spawn(async move {
        let running = server
            .serve(server_transport)
            .await
            .expect("server should handshake");
        running.waiting().await.expect("server should stop cleanly");
    });
    let mut client = ().serve(client_transport).await.expect("client should handshake");

    let mut meta = Meta::new();
    meta.set_traceparent(VALID_TRACEPARENT);
    meta.set_tracestate("vendor=value");
    meta.set_baggage("email=alice@example.com,sessionId=s123");
    let mut request = CallToolRequestParams::new("attacker_tool_name").with_arguments(
        serde_json::Map::from_iter([
            ("action".to_owned(), json!("attacker-action")),
            ("_response_cursor".to_owned(), json!("x".repeat(257))),
            ("_response_offset".to_owned(), json!(1)),
        ]),
    );
    request.meta = Some(meta);

    let error = client
        .call_tool(request)
        .await
        .expect_err("bad paging args should reject before auth");
    let ServiceError::McpError(error) = error else {
        panic!("expected MCP protocol error, got: {error}");
    };
    assert!(error
        .message
        .contains("_response_cursor exceeded 256 bytes"));

    client.close().await.expect("client should close");
    server_handle.await.expect("server task should join");
    drop(guard);

    let logs = buf.contents();
    assert!(
        logs.contains("MCP tool rejected response paging params"),
        "logs were: {logs}"
    );
    assert!(!logs.contains("attacker_tool_name"), "logs were: {logs}");
    assert!(!logs.contains("attacker-action"), "logs were: {logs}");
    assert!(!logs.contains("trace_id_prefix"), "logs were: {logs}");
    assert!(!logs.contains("span_id_prefix"), "logs were: {logs}");
    assert!(!logs.contains("trace_invalid"), "logs were: {logs}");
    assert!(!logs.contains(VALID_TRACEPARENT), "logs were: {logs}");
    assert!(!logs.contains("vendor=value"), "logs were: {logs}");
    assert!(!logs.contains("alice@example.com"), "logs were: {logs}");
    assert!(!logs.contains("s123"), "logs were: {logs}");
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
