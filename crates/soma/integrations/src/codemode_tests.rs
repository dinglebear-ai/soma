use serde_json::json;
use soma_application::{CodeModeExecuteRequest, ExecutionContext};
use soma_domain::{RequestId, Surface};

use super::*;

fn context() -> ExecutionContext {
    ExecutionContext::loopback(Surface::Mcp, RequestId::new("codemode-test").unwrap())
}

#[tokio::test]
async fn executes_the_requested_snippet_and_returns_its_result() {
    let port = CodeModeApplicationPort::new(CodeModeConfig {
        enabled: true,
        ..CodeModeConfig::default()
    });
    let request = CodeModeExecuteRequest {
        source: "return 1 + 1;".to_owned(),
        input: json!({}),
    };

    let output = port
        .execute(request, &context())
        .await
        .expect("snippet executes");

    assert_eq!(output["result"], json!(2));
}

#[tokio::test]
async fn surfaces_snippet_errors_as_a_port_error() {
    let port = CodeModeApplicationPort::new(CodeModeConfig {
        enabled: true,
        ..CodeModeConfig::default()
    });
    let request = CodeModeExecuteRequest {
        source: "throw new Error('boom');".to_owned(),
        input: json!({}),
    };

    let error = port
        .execute(request, &context())
        .await
        .expect_err("snippet error surfaces");

    // Thrown JS errors surface through the runner as `ToolError::Sdk`, whose
    // `kind()` is the runner-supplied `sdk_kind` string — distinct from the
    // caller-mistake and authorization codes covered below, and retryable
    // since it reflects a runner-side failure rather than a bad snippet.
    assert!(error.code.starts_with("codemode_"));
    assert_ne!(error.code, "codemode_disabled");
    assert!(error.retryable);
}

#[tokio::test]
async fn disabled_config_is_rejected_before_running_the_snippet() {
    let port = CodeModeApplicationPort::new(CodeModeConfig {
        enabled: false,
        ..CodeModeConfig::default()
    });
    let request = CodeModeExecuteRequest {
        source: "return 1 + 1;".to_owned(),
        input: json!({}),
    };

    let error = port
        .execute(request, &context())
        .await
        .expect_err("disabled config is rejected");

    assert_eq!(error.code, "codemode_disabled");
    assert!(!error.retryable);
}

#[tokio::test]
async fn default_port_is_disabled() {
    let port = CodeModeApplicationPort::default();
    let request = CodeModeExecuteRequest {
        source: "return 1 + 1;".to_owned(),
        input: json!({}),
    };

    let error = port
        .execute(request, &context())
        .await
        .expect_err("default config is disabled");

    assert_eq!(error.code, "codemode_disabled");
}
