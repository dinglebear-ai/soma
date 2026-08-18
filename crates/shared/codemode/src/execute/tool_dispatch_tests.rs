use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::CodeModeConfig;
use crate::host::{
    CodeModeHost, ExecCtx, HostFuture, ResolvedSnippet, ToolCallOutcome, ToolsRender,
};
use crate::types::{CodeModeCaller, CodeModeSurface, ToolDescriptor, ToolScope, UiLink};

use super::budget::RunBudget;
use super::tool_dispatch::{ToolCallContext, handle_tool_call, local_providers_allowed};

#[test]
fn local_providers_require_unscoped_admin_or_trusted_local() {
    let caller = CodeModeCaller::trusted_local("local");
    assert!(local_providers_allowed(&caller, &ToolScope::All));
    assert!(!local_providers_allowed(
        &caller,
        &ToolScope::Namespaces(["state".to_string()].into_iter().collect())
    ));
}

#[derive(Debug)]
struct UiHost;

impl CodeModeHost for UiHost {
    fn list_tools<'a>(
        &'a self,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        include_snippets: bool,
        use_cache: bool,
    ) -> HostFuture<'a, Result<ToolsRender, crate::ToolError>> {
        let _ = (caller, surface, scope, include_snippets, use_cache);
        Box::pin(async { Ok(ToolsRender::empty()) })
    }

    fn call_tool<'a>(
        &'a self,
        _id: &'a str,
        _params: Value,
        _caller: &'a CodeModeCaller,
        _surface: CodeModeSurface,
        _scope: &'a ToolScope,
        _ctx: ExecCtx,
    ) -> HostFuture<'a, Result<ToolCallOutcome, crate::ToolError>> {
        Box::pin(async {
            Ok(ToolCallOutcome {
                value: json!({"ok": true}),
                ui: Some(UiLink {
                    url: "ui://demo/widget.html".to_string(),
                    title: Some("Demo widget".to_string()),
                }),
            })
        })
    }

    fn resolve_snippet<'a>(
        &'a self,
        name: &'a str,
        _input: Value,
    ) -> HostFuture<'a, Result<ResolvedSnippet, crate::ToolError>> {
        Box::pin(async move {
            Err(crate::ToolError::UnknownInstance {
                message: format!("unknown snippet {name}"),
                valid: Vec::new(),
            })
        })
    }
}

#[tokio::test]
async fn nested_tool_ui_is_retained_on_the_executed_call() {
    let host = UiHost;
    let entries = [ToolDescriptor::tool("demo", "widget", "", None, None)];
    let caller = CodeModeCaller::trusted_local("test");
    let scope = ToolScope::All;
    let execution_id: Option<Arc<str>> = None;
    let ui_capture = Arc::new(Mutex::new(None));
    let mut calls = Vec::new();
    let mut budget = RunBudget::new(&CodeModeConfig::default());
    let value = {
        let mut context = ToolCallContext {
            host: Some(&host),
            entries: &entries,
            caller: &caller,
            surface: CodeModeSurface::Cli,
            scope: &scope,
            execution_id: &execution_id,
            ui_capture: &ui_capture,
            calls: &mut calls,
        };
        handle_tool_call(
            &mut context,
            &mut budget,
            0,
            "demo::widget".to_string(),
            json!({}),
        )
        .await
        .unwrap()
    };

    assert_eq!(value, json!({"ok": true}));
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].ui.as_ref().map(|ui| ui.url.as_str()),
        Some("ui://demo/widget.html")
    );
    assert_eq!(
        ui_capture
            .lock()
            .unwrap()
            .as_ref()
            .map(|ui| ui.url.as_str()),
        Some("ui://demo/widget.html")
    );
}
