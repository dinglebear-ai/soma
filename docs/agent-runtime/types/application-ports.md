---
title: "Agent Runtime Application Ports"
created: 2026-08-05
updated: 2026-08-05
doc_type: "types"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Application Ports

Proposed file family: <code>crates/soma/application/src/agent_runtime/</code>.

The existing <code>ApplicationPorts</code> pattern should be extended with one product-facing <code>AgentRuntimePorts</code> aggregate rather than adding every adapter directly to <code>ApplicationPorts</code>.

~~~rust
use async_trait::async_trait;
use std::sync::Arc;

use soma_domain::agent_runtime::{
    AgentRun, AgentStack, CanonicalRef, CompileContextRequest, CompiledContext,
    DisclosureDecision, DisclosureRequest, EffectiveCapabilities, RunId,
    SnippetDefinition, SnippetExecutionResult, SynthesisRequest, SynthesisResult,
};

use crate::{ApplicationError, ExecutionContext};

#[async_trait]
pub trait AgentStackStore: Send + Sync {
    async fn save_resolved(
        &self,
        stack: ResolvedAgentStack,
        context: &ExecutionContext,
    ) -> Result<CanonicalRef, ApplicationError>;

    async fn get_resolved(
        &self,
        reference: &CanonicalRef,
        context: &ExecutionContext,
    ) -> Result<ResolvedAgentStack, ApplicationError>;
}

#[async_trait]
pub trait ContextCompilerPort: Send + Sync {
    async fn validate(
        &self,
        manifest: ContextManifestInput,
        context: &ExecutionContext,
    ) -> Result<ContextValidationReport, ApplicationError>;

    async fn compile(
        &self,
        request: CompileContextRequest,
        context: &ExecutionContext,
    ) -> Result<CompiledContext, ApplicationError>;

    async fn enrich(
        &self,
        request: EnrichContextRequest,
        context: &ExecutionContext,
    ) -> Result<CompiledContext, ApplicationError>;
}

#[async_trait]
pub trait CompiledContextStore: Send + Sync {
    async fn publish(
        &self,
        compiled: CompiledContext,
        context: &ExecutionContext,
    ) -> Result<CanonicalRef, ApplicationError>;

    async fn get(
        &self,
        reference: &CanonicalRef,
        context: &ExecutionContext,
    ) -> Result<CompiledContext, ApplicationError>;
}

#[async_trait]
pub trait DisclosurePort: Send + Sync {
    async fn decide(
        &self,
        request: DisclosureRequest,
        context: &ExecutionContext,
    ) -> Result<DisclosureDecision, ApplicationError>;
}

#[async_trait]
pub trait SnippetCatalogPort: Send + Sync {
    async fn list(
        &self,
        request: ListSnippetsRequest,
        context: &ExecutionContext,
    ) -> Result<Vec<SnippetDefinition>, ApplicationError>;

    async fn resolve(
        &self,
        request: ResolveSnippetRequest,
        context: &ExecutionContext,
    ) -> Result<ResolvedSnippetDefinition, ApplicationError>;
}

#[async_trait]
pub trait SnippetExecutionPort: Send + Sync {
    async fn execute(
        &self,
        request: ExecuteSnippetRequest,
        context: &ExecutionContext,
    ) -> Result<SnippetExecutionResult, ApplicationError>;
}

#[async_trait]
pub trait PackageManagerPort: Send + Sync {
    async fn resolve(
        &self,
        request: ResolvePackageRequest,
        context: &ExecutionContext,
    ) -> Result<ResolvedPackage, ApplicationError>;
}

#[async_trait]
pub trait GatewayLoadoutPort: Send + Sync {
    async fn resolve(
        &self,
        request: ResolveLoadoutRequest,
        context: &ExecutionContext,
    ) -> Result<EffectiveCapabilities, ApplicationError>;

    async fn release(
        &self,
        run_id: &RunId,
        context: &ExecutionContext,
    ) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait AgentRuntimePort: Send + Sync {
    async fn provision(
        &self,
        request: ProvisionRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<ProvisionedRuntime, ApplicationError>;

    async fn execute(
        &self,
        request: ExecuteAgentRequest,
        context: &ExecutionContext,
    ) -> Result<AgentExecutionResult, ApplicationError>;

    async fn stop(
        &self,
        request: StopRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<RuntimeFinalization, ApplicationError>;
}

#[async_trait]
pub trait SynthesisPort: Send + Sync {
    async fn synthesize(
        &self,
        request: SynthesisRequest,
        context: &ExecutionContext,
    ) -> Result<SynthesisResult, ApplicationError>;
}

#[async_trait]
pub trait LifecycleEventPort: Send + Sync {
    async fn publish(
        &self,
        events: Vec<LifecycleEvent>,
        context: &ExecutionContext,
    ) -> Result<(), ApplicationError>;
}

#[derive(Clone)]
pub struct AgentRuntimePorts {
    pub stack_store: Arc<dyn AgentStackStore>,
    pub context_compiler: Arc<dyn ContextCompilerPort>,
    pub context_store: Arc<dyn CompiledContextStore>,
    pub disclosure: Arc<dyn DisclosurePort>,
    pub snippets: Arc<dyn SnippetCatalogPort>,
    pub snippet_execution: Arc<dyn SnippetExecutionPort>,
    pub packages: Arc<dyn PackageManagerPort>,
    pub gateway: Arc<dyn GatewayLoadoutPort>,
    pub runtime: Arc<dyn AgentRuntimePort>,
    pub synthesis: Arc<dyn SynthesisPort>,
    pub events: Arc<dyn LifecycleEventPort>,
}
~~~

## Application use cases

Add methods to <code>SomaApplication</code> or a composed <code>AgentRuntimeApplication</code> for:

- validate and resolve stack;
- create, get, list, cancel, retry, and approve runs;
- compile, inspect, compare, enrich, and materialize context;
- list, resolve, execute, promote, and remove snippets;
- request and resolve disclosure;
- resolve and inspect loadouts;
- execute synthesis;
- list outputs and artifacts.

Every method receives the existing <code>ExecutionContext</code> so surface, principal, request ID, authorization, and trace behavior remain unified.
