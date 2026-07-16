use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::error::ToolError;
use crate::types::{CodeModeCaller, CodeModeSurface, ToolDescriptor, ToolScope, UiLink};

pub type HostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone)]
pub struct ToolsRender {
    pub fingerprint: String,
    pub entries: Arc<[ToolDescriptor]>,
    pub catalog_json: Arc<str>,
    pub serialized_size: usize,
}

impl ToolsRender {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            fingerprint: String::new(),
            entries: Arc::from([]),
            catalog_json: Arc::from("[]"),
            serialized_size: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedSnippet {
    pub name: String,
    pub code: String,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct ToolCallOutcome {
    pub value: Value,
    pub ui: Option<UiLink>,
}

#[derive(Debug, Clone)]
pub struct ExecCtx {
    pub seq: u64,
    pub execution_id: Option<Arc<str>>,
    pub step_ordinal: Option<u64>,
}

impl ExecCtx {
    #[must_use]
    pub fn none() -> Self {
        Self {
            seq: 0,
            execution_id: None,
            step_ordinal: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepDecision {
    Replay(Value),
    Execute,
    Error { kind: String, message: String },
}

pub trait CodeModeHost: Send + Sync {
    fn list_tools<'a>(
        &'a self,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        include_snippets: bool,
        use_cache: bool,
    ) -> HostFuture<'a, Result<ToolsRender, ToolError>>;

    fn call_tool<'a>(
        &'a self,
        id: &'a str,
        params: Value,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        ctx: ExecCtx,
    ) -> HostFuture<'a, Result<ToolCallOutcome, ToolError>>;

    fn resolve_snippet<'a>(
        &'a self,
        name: &'a str,
        input: Value,
    ) -> HostFuture<'a, Result<ResolvedSnippet, ToolError>>;

    fn semantic_search<'a>(
        &'a self,
        query: &'a str,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        top_k: usize,
    ) -> HostFuture<'a, Result<Vec<(String, f32)>, ToolError>> {
        let _ = (query, caller, surface, scope, top_k);
        Box::pin(async { Ok(Vec::new()) })
    }

    fn decide_step<'a>(&'a self, ctx: ExecCtx, name: &'a str) -> HostFuture<'a, StepDecision> {
        let _ = (ctx, name);
        Box::pin(async { StepDecision::Execute })
    }

    fn record_step<'a>(
        &'a self,
        ctx: ExecCtx,
        name: &'a str,
        value: &'a Value,
    ) -> HostFuture<'a, Result<(), ToolError>> {
        let _ = (ctx, name, value);
        Box::pin(async { Ok(()) })
    }

    #[cfg(feature = "openapi")]
    fn openapi_registry(&self) -> Option<soma_openapi::OpenApiRegistry> {
        None
    }

    #[cfg(feature = "openapi")]
    fn openapi_http_client(&self) -> Option<reqwest::Client> {
        None
    }
}

#[derive(Debug, Default)]
pub struct NoopHost;

impl CodeModeHost for NoopHost {
    fn list_tools<'a>(
        &'a self,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        include_snippets: bool,
        use_cache: bool,
    ) -> HostFuture<'a, Result<ToolsRender, ToolError>> {
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
        ctx: ExecCtx,
    ) -> HostFuture<'a, Result<ToolCallOutcome, ToolError>> {
        let _ = (params, caller, surface, scope, ctx);
        Box::pin(async move {
            Err(ToolError::UnknownAction {
                message: format!("unknown Code Mode tool `{id}`"),
                valid: Vec::new(),
                hint: None,
            })
        })
    }

    fn resolve_snippet<'a>(
        &'a self,
        name: &'a str,
        input: Value,
    ) -> HostFuture<'a, Result<ResolvedSnippet, ToolError>> {
        let _ = input;
        Box::pin(async move {
            Err(ToolError::UnknownInstance {
                message: format!("unknown Code Mode snippet `{name}`"),
                valid: Vec::new(),
            })
        })
    }
}
