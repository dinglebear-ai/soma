use super::broker::{code_mode_unknown_tool_hint, CodeModeBroker};
use super::host::NoopHost;
#[cfg(feature = "openapi")]
use super::{
    host::{CodeModeHost, HostFuture, ToolCallOutcome, ToolsRender},
    types::{CodeModeCaller, CodeModeSurface, ToolScope},
    CodeModeConfig,
};
#[cfg(feature = "openapi")]
use serde_json::Value;

#[test]
fn broker_keeps_run_scoped_ui_capture() {
    let host = NoopHost;
    let broker = CodeModeBroker::new(Some(&host));
    assert!(broker.ui_capture.lock().unwrap().is_none());
}

#[test]
fn unknown_tool_hint_points_to_codemode_discovery() {
    assert!(code_mode_unknown_tool_hint().contains("codemode.search"));
}

#[cfg(feature = "openapi")]
#[derive(Debug)]
struct EmptyOpenApiHost;

#[cfg(feature = "openapi")]
impl CodeModeHost for EmptyOpenApiHost {
    fn list_tools<'a>(
        &'a self,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        include_snippets: bool,
        use_cache: bool,
    ) -> HostFuture<'a, Result<ToolsRender, super::ToolError>> {
        let _ = (caller, surface, scope, include_snippets, use_cache);
        Box::pin(async { Ok(ToolsRender::empty()) })
    }

    fn call_tool<'a>(
        &'a self,
        id: &'a str,
        params: Value,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        ctx: super::host::ExecCtx,
    ) -> HostFuture<'a, Result<ToolCallOutcome, super::ToolError>> {
        let _ = (params, caller, surface, scope, ctx);
        Box::pin(async move {
            Err(super::ToolError::UnknownAction {
                message: format!("unknown tool `{id}`"),
                valid: Vec::new(),
                hint: None,
            })
        })
    }

    fn resolve_snippet<'a>(
        &'a self,
        name: &'a str,
        input: Value,
    ) -> HostFuture<'a, Result<super::host::ResolvedSnippet, super::ToolError>> {
        let _ = input;
        Box::pin(async move {
            Err(super::ToolError::UnknownInstance {
                message: format!("unknown snippet `{name}`"),
                valid: Vec::new(),
            })
        })
    }

    fn openapi_registry(&self) -> Option<soma_openapi::OpenApiRegistry> {
        Some(soma_openapi::OpenApiRegistry::default())
    }

    fn openapi_http_client(&self) -> Option<reqwest::Client> {
        Some(soma_openapi::http::build_dispatch_client().unwrap())
    }
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn broker_routes_openapi_calls_to_feature_dispatcher() {
    let host = EmptyOpenApiHost;
    let broker = CodeModeBroker::new(Some(&host));
    let error = broker
        .execute(
            r#"async () => openapi.call("vendor", "getUser", {})"#,
            CodeModeCaller::trusted_local("test"),
            CodeModeSurface::Cli,
            CodeModeConfig::default(),
            ToolScope::All,
            None,
        )
        .await
        .unwrap_err();

    assert_eq!(error.kind(), "unknown_instance", "{}", error.user_message());
    assert!(error.user_message().contains("OpenAPI"));
}
