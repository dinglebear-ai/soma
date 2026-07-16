use crate::config::UpstreamConfig;
use crate::upstream::pool::{InProcessUpstream, ToolCall, UpstreamPool};
use crate::upstream::{ToolDescriptor, UpstreamError};

#[tokio::test]
async fn routed_tool_call_uses_in_process_upstream() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "local".to_owned(),
        ..UpstreamConfig::default()
    };
    let upstream = InProcessUpstream::new("local").with_tool(
        ToolDescriptor::new("echo"),
        serde_json::json!({"echo": "ok"}),
    );
    pool.register_in_process(config, upstream).unwrap();

    let result = pool
        .call_tool(ToolCall {
            upstream: "local".to_owned(),
            tool: "echo".to_owned(),
            params: serde_json::json!({}),
        })
        .await
        .unwrap();

    assert_eq!(result, serde_json::json!({"echo": "ok"}));
}

#[tokio::test]
async fn exposed_tool_filter_fails_closed() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "local".to_owned(),
        expose_tools: Some(vec!["safe_*".to_owned()]),
        ..UpstreamConfig::default()
    };
    let upstream = InProcessUpstream::new("local")
        .with_tool(ToolDescriptor::new("delete"), serde_json::json!({}));
    pool.register_in_process(config, upstream).unwrap();

    let error = pool
        .call_tool(ToolCall {
            upstream: "local".to_owned(),
            tool: "delete".to_owned(),
            params: serde_json::json!({}),
        })
        .await
        .unwrap_err();

    assert!(matches!(error, UpstreamError::NotExposed { .. }));
}

#[tokio::test]
async fn params_must_be_objects() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "local".to_owned(),
        ..UpstreamConfig::default()
    };
    let upstream = InProcessUpstream::new("local")
        .with_tool(ToolDescriptor::new("echo"), serde_json::json!({}));
    pool.register_in_process(config, upstream).unwrap();

    let error = pool
        .call_tool(ToolCall {
            upstream: "local".to_owned(),
            tool: "echo".to_owned(),
            params: serde_json::json!("not-object"),
        })
        .await
        .unwrap_err();

    assert_eq!(error, UpstreamError::ParamsMustBeObject);
}
