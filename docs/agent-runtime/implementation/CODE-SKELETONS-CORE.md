---
title: "Agent Runtime Core Code Skeletons"
created: 2026-08-05
updated: 2026-08-05
doc_type: "implementation-plan"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Core Code Skeletons

## 1. Canonical appdata paths

Proposed file: <code>crates/soma/config/src/agent_runtime_paths.rs</code>.

~~~rust
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::default_data_dir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRuntimePaths {
    root: PathBuf,
}

impl AgentRuntimePaths {
    pub fn from_default_data_dir() -> Result<Self> {
        Self::new(default_data_dir()?)
    }

    pub fn new(root: PathBuf) -> Result<Self> {
        if !root.is_absolute() {
            bail!("Soma data root must be absolute: {}", root.display());
        }
        if root.as_os_str().is_empty() {
            bail!("Soma data root must not be empty");
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path { &self.root }
    pub fn config(&self) -> PathBuf { self.root.join("config.toml") }
    pub fn providers(&self) -> PathBuf { self.root.join("providers") }
    pub fn snippets(&self) -> PathBuf { self.root.join("snippets") }
    pub fn stacks(&self) -> PathBuf { self.root.join("stacks") }
    pub fn contexts(&self) -> PathBuf { self.root.join("contexts") }
    pub fn context_manifests(&self) -> PathBuf { self.contexts().join("manifests") }
    pub fn compiled_contexts(&self) -> PathBuf { self.contexts().join("compiled") }
    pub fn loadouts(&self) -> PathBuf { self.root.join("loadouts") }
    pub fn packages(&self) -> PathBuf { self.root.join("packages") }
    pub fn package_cache(&self) -> PathBuf { self.packages().join("cache") }
    pub fn runs(&self) -> PathBuf { self.root.join("runs") }
    pub fn run(&self, run_id: &str) -> PathBuf { self.runs().join(run_id) }
    pub fn logs(&self) -> PathBuf { self.root.join("logs") }
    pub fn cache(&self) -> PathBuf { self.root.join("cache") }

    pub fn ensure_runtime_dirs(&self) -> Result<()> {
        for path in [
            self.snippets(),
            self.stacks(),
            self.context_manifests(),
            self.compiled_contexts(),
            self.loadouts(),
            self.package_cache(),
            self.runs(),
            self.logs(),
            self.cache(),
        ] {
            std::fs::create_dir_all(&path)
                .with_context(|| format!("failed to create {}", path.display()))?;
            reject_symlink(&path)?;
            secure_directory(&path)?;
        }
        Ok(())
    }
}

fn reject_symlink(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("runtime path must be a real directory: {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn secure_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to secure {}", path.display()))
}

#[cfg(not(unix))]
fn secure_directory(_path: &Path) -> Result<()> { Ok(()) }
~~~

Update <code>Config::load()</code> candidate order:

~~~rust
let mut paths = Vec::new();
if let Ok(data_dir) = default_data_dir() {
    paths.push(data_dir.join("config.toml"));
}
paths.push(std::path::PathBuf::from("config.toml"));
~~~

Update provider default in <code>apps/soma/src/bootstrap.rs</code>:

~~~rust
let default_provider_dir = std::env::var_os("SOMA_PROVIDER_DIR")
    .map(std::path::PathBuf::from)
    .map(Ok)
    .unwrap_or_else(|| {
        soma_config::AgentRuntimePaths::from_default_data_dir()
            .map(|paths| paths.providers())
    })?;
~~~

## 2. Application port bundle

Proposed file: <code>crates/soma/application/src/agent_runtime/ports.rs</code>.

~~~rust
use std::sync::Arc;

use async_trait::async_trait;

use soma_domain::agent_runtime::{
    AgentRun, CompileContextRequest, CompiledContext, DisclosureDecision,
    DisclosureRequest, EffectiveCapabilities, RunId, SnippetExecutionResult,
    SynthesisRequest, SynthesisResult,
};

use crate::{ApplicationError, ExecutionContext};

#[async_trait]
pub trait AgentRunPort: Send + Sync {
    async fn create(
        &self,
        request: CreateAgentRunRequest,
        context: &ExecutionContext,
    ) -> Result<AgentRun, ApplicationError>;

    async fn get(
        &self,
        run_id: &RunId,
        context: &ExecutionContext,
    ) -> Result<AgentRun, ApplicationError>;

    async fn transition(
        &self,
        request: TransitionAgentRunRequest,
        context: &ExecutionContext,
    ) -> Result<AgentRun, ApplicationError>;
}

#[async_trait]
pub trait ContextCompilerPort: Send + Sync {
    async fn compile(
        &self,
        request: CompileContextRequest,
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
pub trait LoadoutPort: Send + Sync {
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
pub trait AgentExecutorPort: Send + Sync {
    async fn provision(
        &self,
        request: ProvisionAgentRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<ProvisionedAgentRuntime, ApplicationError>;

    async fn execute(
        &self,
        request: ExecuteAgentRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<AgentExecutionResult, ApplicationError>;

    async fn finalize(
        &self,
        request: FinalizeAgentRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<RuntimeFinalization, ApplicationError>;
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
pub trait SynthesisPort: Send + Sync {
    async fn synthesize(
        &self,
        request: SynthesisRequest,
        context: &ExecutionContext,
    ) -> Result<SynthesisResult, ApplicationError>;
}

#[derive(Clone)]
pub struct AgentRuntimePorts {
    pub runs: Arc<dyn AgentRunPort>,
    pub contexts: Arc<dyn ContextCompilerPort>,
    pub disclosure: Arc<dyn DisclosurePort>,
    pub loadouts: Arc<dyn LoadoutPort>,
    pub executor: Arc<dyn AgentExecutorPort>,
    pub snippets: Arc<dyn SnippetExecutionPort>,
    pub synthesis: Arc<dyn SynthesisPort>,
}

impl AgentRuntimePorts {
    pub fn unavailable() -> Self {
        let unavailable = Arc::new(UnavailableAgentRuntimePort);
        Self {
            runs: unavailable.clone(),
            contexts: unavailable.clone(),
            disclosure: unavailable.clone(),
            loadouts: unavailable.clone(),
            executor: unavailable.clone(),
            snippets: unavailable.clone(),
            synthesis: unavailable,
        }
    }
}

struct UnavailableAgentRuntimePort;

fn unavailable() -> ApplicationError {
    ApplicationError::from_code(
        "engine_unavailable",
        "agent runtime is not configured for this application instance",
    )
}
~~~

Implement every trait for <code>UnavailableAgentRuntimePort</code> with the same error. Use the repository's actual <code>ApplicationError</code> constructor rather than adding <code>from_code</code> if it does not exist.

Extend current <code>ApplicationPorts</code>:

~~~rust
pub struct ApplicationPorts {
    pub gateway: Arc<dyn GatewayPort>,
    pub codemode: Arc<dyn CodeModePort>,
    pub openapi: Arc<dyn OpenApiPort>,
    pub python_environment: Arc<dyn PythonEnvironmentPort>,
    pub agent_runtime: AgentRuntimePorts,
}

pub fn with_agent_runtime(mut self, agent_runtime: AgentRuntimePorts) -> Self {
    self.agent_runtime = agent_runtime;
    self
}
~~~

## 3. Stack resolution use case

Proposed file: <code>crates/soma/application/src/agent_runtime/stack.rs</code>.

~~~rust
pub async fn resolve_stack(
    ports: &AgentRuntimePorts,
    request: ResolveAgentStackRequest,
    context: &ExecutionContext,
) -> Result<ResolvedAgentStack, ApplicationError> {
    let source = request.source.load_and_validate()?;

    if source.services.len() != 1 {
        return Err(ApplicationError::invalid_request(
            "multi_service_not_supported",
            "the first agent-runtime slice supports exactly one service",
        ));
    }

    let package = ports.packages.resolve(
        ResolvePackageRequest::from_stack(&source),
        context,
    ).await?;

    let context_manifest = ports.context_manifests.resolve(
        ResolveContextManifestRequest::from_stack(&source),
        context,
    ).await?;

    let snippets = ports.snippet_catalog.resolve_requirements(
        ResolveSnippetRequirementsRequest::from_stack(&source, &package),
        context,
    ).await?;

    let loadout = ports.loadouts.preview(
        PreviewLoadoutRequest::from_stack(&source, &package, &context_manifest, &snippets),
        context,
    ).await?;

    let runtime = ports.runtime_catalog.resolve(
        ResolveRuntimeSpecRequest::from_stack(&source),
        context,
    ).await?;

    let resolved = ResolvedAgentStack::new(
        source,
        package,
        context_manifest,
        snippets,
        loadout,
        runtime,
    )?;

    ports.stack_store.publish(resolved.clone(), context).await?;
    Ok(resolved)
}
~~~

The actual <code>AgentRuntimePorts</code> may group package, manifest, catalog, and store ports differently. Preserve the ordering: all resolution completes before provisioning.

## 4. Durable run transition

Proposed application method:

~~~rust
pub async fn transition_agent_run(
    &self,
    request: TransitionAgentRunRequest,
    context: ExecutionContext,
) -> Result<AgentRun, ApplicationError> {
    let current = self.ports.agent_runtime.runs
        .get(&request.run_id, &context)
        .await?;

    if current.state_version != request.expected_state_version {
        return Err(ApplicationError::conflict(
            "agent_run_state_conflict",
            "agent run state changed; reload and retry",
        ));
    }
    if !current.state.can_transition_to(request.next_state) {
        return Err(ApplicationError::conflict(
            "agent_run_transition_invalid",
            format!("cannot transition from {:?} to {:?}", current.state, request.next_state),
        ));
    }

    self.ports.agent_runtime.runs
        .transition(request, &context)
        .await
}
~~~

The store implementation must update state and insert the lifecycle-outbox row in one transaction with <code>WHERE state_version = ?</code> optimistic concurrency.

## 5. Snippet requirement resolution

Proposed shared Code Mode helper:

~~~rust
pub fn resolve_requirements(
    snippet: &SnippetDefinition,
    skills: &ResolvedPrimitiveCatalog,
    context_domains: &BTreeSet<String>,
    tools: &BTreeSet<String>,
    snippets: &SnippetIndex,
    max_mutation: MutationClass,
) -> Result<ResolvedSnippetRequirements, ToolError> {
    if snippet.risk_class > max_mutation {
        return Err(ToolError::Sdk {
            sdk_kind: "snippet_risk_denied".into(),
            message: format!(
                "snippet requires {:?}, but the run allows {:?}",
                snippet.risk_class, max_mutation
            ),
        });
    }

    let missing_skills = snippet.skills.iter()
        .filter(|required| !skills.satisfies(required))
        .cloned()
        .collect::<Vec<_>>();
    let missing_context = snippet.context_domains.iter()
        .filter(|required| !context_domains.contains(*required))
        .cloned()
        .collect::<Vec<_>>();
    let missing_tools = snippet.tools.iter()
        .filter(|required| !tools.contains(*required))
        .cloned()
        .collect::<Vec<_>>();
    let missing_snippets = snippet.snippets.iter()
        .filter(|required| !snippets.satisfies(required))
        .cloned()
        .collect::<Vec<_>>();

    if !(missing_skills.is_empty()
        && missing_context.is_empty()
        && missing_tools.is_empty()
        && missing_snippets.is_empty())
    {
        return Err(ToolError::Sdk {
            sdk_kind: "snippet_requirement_missing".into(),
            message: serde_json::json!({
                "skills": missing_skills,
                "context": missing_context,
                "tools": missing_tools,
                "snippets": missing_snippets,
            }).to_string(),
        });
    }

    Ok(ResolvedSnippetRequirements {
        risk_class: snippet.risk_class,
        skills: snippet.skills.clone(),
        context_domains: snippet.context_domains.clone(),
        tools: snippet.tools.clone(),
        snippets: snippet.snippets.clone(),
    })
}
~~~

## 6. Context compilation orchestration

Proposed file: <code>crates/soma/application/src/agent_runtime/context/compiler.rs</code>.

~~~rust
pub async fn compile_context(
    ports: &ContextPorts,
    request: CompileContextRequest,
    context: &ExecutionContext,
) -> Result<CompiledContext, ApplicationError> {
    let manifest = ports.manifests.resolve(&request.manifest, context).await?;
    let selected = manifest.select_view(request.view.as_deref())?;
    let roots = ports.roots.resolve(&selected.roots, &request, context).await?;
    let sources = ports.sources.inspect(&selected.sources, &request, context).await?;

    enforce_required_sources(&sources)?;
    let plan = ports.planner.plan(ContextPlanRequest {
        request: request.clone(),
        manifest: manifest.clone(),
        roots,
        sources,
        authorization: context.authorization.clone(),
    }, context).await?;

    let candidates = ports.query.execute(&plan, context).await?;
    let authorized = ports.authorization.filter_candidates(candidates, context).await?;
    let fused = ports.fusion.fuse(authorized, &plan, context).await?;
    let hydrated = ports.evidence.hydrate(fused, context).await?;
    let selected_items = enforce_budgets_and_order(hydrated, &plan.budgets)?;

    let compiled = CompiledContextBuilder::new(request, manifest, plan)
        .items(selected_items.items)
        .source_reports(selected_items.sources)
        .conflicts(selected_items.conflicts)
        .budget(selected_items.budget)
        .build()?;

    ports.store.publish(compiled.clone(), context).await?;
    Ok(compiled)
}
~~~

Authorization must filter each retrieval lane before fusion. The <code>authorization.filter_candidates</code> line is defense in depth, not the only filter.
