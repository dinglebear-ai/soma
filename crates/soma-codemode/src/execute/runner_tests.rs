use serde_json::json;
use std::sync::Arc;

use crate::host::NoopHost;
use crate::types::{
    CodeModeCaller, CodeModeCatalogKind, CodeModeSurface, ToolDescriptor, ToolScope,
};
use crate::CodeModeConfig;

use super::runner::{
    build_proxy, execute_in_subprocess, local_providers_allowed, SubprocessExecution,
};

#[test]
fn proxy_exposes_discovery_and_helpers() {
    let descriptor = ToolDescriptor {
        kind: CodeModeCatalogKind::Tool,
        id: "demo::ping".to_string(),
        name: "ping".to_string(),
        namespace: "demo".to_string(),
        description: "Ping".to_string(),
        schema: Some(json!({"type": "object"})),
        output_schema: None,
        signature: "codemode.demo.ping(params?)".to_string(),
        dts: "declare const ping: Function;".to_string(),
        tags: Vec::new(),
        inputs: Vec::new(),
    };
    let proxy = build_proxy(&[descriptor], 0.5).unwrap();
    assert!(proxy.contains("codemode.search"));
    assert!(proxy.contains("codemode.describe"));
    assert!(proxy.contains("codemode.demo.ping"));
    assert!(proxy.contains("codemode.state.readFile"));
}

#[test]
fn local_providers_require_unscoped_admin_or_trusted_local() {
    let caller = CodeModeCaller::trusted_local("local");
    assert!(local_providers_allowed(&caller, &ToolScope::All));
    assert!(!local_providers_allowed(
        &caller,
        &ToolScope::Namespaces(["state".to_string()].into_iter().collect())
    ));
}

#[tokio::test]
async fn subprocess_runner_executes_plain_code_without_host() {
    let outcome = execute_in_subprocess::<NoopHost>(SubprocessExecution {
        host: None,
        code: "async () => ({ answer: 42 })",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config: CodeModeConfig::default(),
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .unwrap();

    assert_eq!(outcome.raw_response.result, Some(json!({"answer": 42})));
    assert!(outcome.raw_response.calls.is_empty());
}
