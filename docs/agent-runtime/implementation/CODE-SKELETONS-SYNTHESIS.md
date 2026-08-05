---
title: "Agent Runtime Synthesis Code Skeletons"
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

# Synthesis and Integration Code Skeletons

## 1. Run-scoped Code Mode host

Proposed adapter module: <code>crates/soma/integrations/src/codemode/context_host.rs</code>.

~~~rust
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use soma_codemode::{
    CodeModeCaller, CodeModeCallerCapabilities, CodeModeHost, CodeModeSourceLookup,
    ToolDescriptor, ToolError, ToolScope, UiLink,
};

#[derive(Clone)]
pub struct RunScopedCodeModeHost {
    run: RunExecutionIdentity,
    tools: Arc<dyn ScopedToolCatalog>,
    snippets: Arc<dyn ResolvedSnippetCatalog>,
    context: Arc<dyn ContextActionPort>,
    research: Arc<dyn ResearchActionPort>,
    steps: Arc<dyn StepReplayPort>,
}

#[async_trait]
impl CodeModeHost for RunScopedCodeModeHost {
    async fn list_tools(&self, scope: &ToolScope) -> Result<Vec<ToolDescriptor>, ToolError> {
        self.tools
            .list_authorized(&self.run, scope)
            .await
            .map_err(tool_error)
    }

    async fn call_tool(
        &self,
        id: &str,
        params: Value,
        caller: &CodeModeCaller,
        scope: &ToolScope,
    ) -> Result<Value, ToolError> {
        enforce_caller_identity(&self.run, caller)?;
        self.tools
            .call_authorized(&self.run, id, params, caller, scope)
            .await
            .map_err(tool_error)
    }

    async fn resolve_snippet(
        &self,
        name: &str,
        input: Value,
        caller: &CodeModeCaller,
        scope: &ToolScope,
    ) -> Result<ResolvedSnippet, ToolError> {
        enforce_caller_identity(&self.run, caller)?;
        self.snippets
            .resolve(&self.run, name, input, scope)
            .await
            .map_err(tool_error)
    }

    async fn lookup_source(
        &self,
        request: CodeModeSourceLookup,
        caller: &CodeModeCaller,
        _scope: &ToolScope,
    ) -> Result<Vec<Value>, ToolError> {
        enforce_caller_identity(&self.run, caller)?;
        self.context
            .semantic_lookup(&self.run, request)
            .await
            .map_err(tool_error)
    }

    async fn replay_step(
        &self,
        execution_id: &str,
        step_name: &str,
        caller: &CodeModeCaller,
    ) -> Result<Option<Value>, ToolError> {
        enforce_caller_identity(&self.run, caller)?;
        self.steps.replay(&self.run, execution_id, step_name).await
    }

    async fn record_step(
        &self,
        execution_id: &str,
        step_name: &str,
        value: Value,
        caller: &CodeModeCaller,
    ) -> Result<(), ToolError> {
        enforce_caller_identity(&self.run, caller)?;
        self.steps
            .record(&self.run, execution_id, step_name, value)
            .await
    }

    fn ui_link(&self, execution_id: &str) -> Option<UiLink> {
        Some(UiLink {
            label: "Inspect Soma agent run".into(),
            href: format!("/runs/{}/codemode/{execution_id}", self.run.run_id),
        })
    }
}

fn caller_for_run(run: &ResolvedAgentRun) -> CodeModeCaller {
    CodeModeCaller {
        caller_id: run.agent_id.to_string(),
        surface: soma_codemode::CodeModeSurface::Internal,
        capabilities: CodeModeCallerCapabilities {
            tools: run.capabilities.tools.iter().cloned().collect(),
            state: run.state_access,
            artifacts: true,
            semantic_search: run.capabilities.context_search,
        },
        execution_id: None,
        step_ordinal: None,
    }
}
~~~

Reconcile exact current Code Mode trait fields and constructors. The invariant is server-side catalog filtering plus run identity on every operation.

## 2. Thread snippet input through the shared protocol

The current Soma application adapter documents that <code>CodeModeExecuteRequest::input</code> is not threaded into snippets. Fix the shared request envelope once.

Proposed protocol extension:

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExecuteInlineRequest {
    pub code: String,
    #[serde(default)]
    pub input: Value,
    pub caller: CodeModeCaller,
    pub scope: ToolScope,
    pub config: CodeModeConfig,
}
~~~

Runner wrapper generation should bind input explicitly:

~~~rust
pub fn wrap_user_code(code: &str, input: &Value) -> Result<String, ToolError> {
    let input = serde_json::to_string(input)
        .map_err(|error| ToolError::invalid_param("input", error.to_string()))?;
    Ok(format!(
        "const input = Object.freeze({input});
const __soma_main = ({code});
await __soma_main();"
    ))
}
~~~

Use the existing normalizer and source-size checks before wrapper generation. Provider Code Mode and application Code Mode must share this path.

## 3. Dependent Axon research adapter

Proposed file: <code>crates/soma/integrations/src/axon_research.rs</code>.

~~~rust
#[derive(Clone)]
pub struct AxonResearchAdapter {
    jobs: Arc<dyn UnifiedJobClient>,
    retrieval: Arc<dyn AxonRetrievalPort>,
    graph: Arc<dyn GraphCandidatePort>,
}

#[async_trait]
impl ResearchActionPort for AxonResearchAdapter {
    async fn create(
        &self,
        request: CreateResearchQuestionRequest,
        context: &ExecutionContext,
    ) -> Result<ResearchQuestion, ApplicationError> {
        if request.depth > request.policy.max_depth {
            return Err(ApplicationError::invalid_request(
                "research_depth_exceeded",
                "dependent research depth exceeds the run policy",
            ));
        }
        if request.derived_from.is_empty() {
            return Err(ApplicationError::invalid_request(
                "research_evidence_required",
                "dependent research must identify the evidence that prompted it",
            ));
        }

        let normalized = normalize_research_question(&request)?;
        let digest = research_question_digest(&normalized)?;
        if let Some(existing) = self.jobs.find_by_dedup_key(&digest, context).await? {
            return map_existing_question(existing);
        }

        let question = ResearchQuestion::new(
            new_research_question_id(),
            request.question,
            request.derived_from,
            request.depth,
        )?;
        persist_question_and_edges(&question, context).await?;

        let retrieval_request = axon_retrieval::RetrievalRequest {
            query: question.question.clone(),
            source_types: request.source_policy.source_types(),
            limit: request.policy.max_documents,
            ..Default::default()
        };
        let ask_context = axon_retrieval::AskContext::from_request(
            &retrieval_request,
            request.policy.user_instructions.clone(),
        );

        let job = self.jobs.create(UnifiedJobRequest {
            kind: "soma-dependent-research".into(),
            dedup_key: Some(digest),
            payload: serde_json::to_value(DependentResearchPayload {
                question_id: question.id.clone(),
                run_id: request.run_id,
                context_generation_id: request.context_generation_id,
                retrieval_request,
                ask_context,
                source_policy: request.source_policy,
                output_schema: request.output_schema,
            })?,
            deadline: request.deadline,
            priority: request.priority,
        }, context).await?;

        attach_job(&question.id, &job, context).await?;
        Ok(question.with_job(job.canonical_ref()))
    }
}
~~~

Use actual context-v1 shared crate names when Axon functionality is transplanted. Soma should not runtime-depend on the standalone Axon product binary for in-process mode unless remote composition is deliberately selected.

## 4. Research worker and child context generation

~~~rust
pub async fn execute_dependent_research_job(
    payload: DependentResearchPayload,
    ports: &DependentResearchPorts,
    context: &ExecutionContext,
) -> Result<DependentResearchOutput, ApplicationError> {
    let plan = ports.retrieval.plan(&payload.retrieval_request, context).await?;
    let bundle = ports.retrieval.retrieve(&plan, context).await?;
    let hydrated = ports.evidence.hydrate_bundle(bundle, context).await?;

    let synthesis = ports.synthesis
        .synthesize_axon_context(
            payload.ask_context,
            hydrated.clone(),
            payload.output_schema,
            context,
        )
        .await?;

    let citations = validate_citations(&synthesis, &hydrated)?;
    let findings = classify_research_findings(synthesis, citations)?;
    let graph_candidates = findings_to_graph_candidates(
        &payload.question_id,
        &findings,
        &hydrated,
    )?;
    ports.graph.publish_candidates(graph_candidates, context).await?;

    let child_context = ports.contexts
        .enrich(EnrichContextRequest {
            parent_generation_id: payload.context_generation_id,
            additions: findings.iter().map(ResearchFinding::canonical_ref).collect(),
            reason: ContextEnrichmentReason::DependentResearch {
                question_id: payload.question_id.clone(),
            },
        }, context)
        .await?;

    Ok(DependentResearchOutput {
        question_id: payload.question_id,
        findings,
        child_context_generation_id: child_context.generation_id,
    })
}
~~~

## 5. Structured synthesis investigation loop

Proposed application file: <code>crates/soma/application/src/agent_runtime/synthesis/investigation.rs</code>.

~~~rust
pub async fn run_synthesis_investigation(
    request: SynthesisRequest,
    ports: &SynthesisPorts,
    context: &ExecutionContext,
) -> Result<SynthesisResult, ApplicationError> {
    let mut investigation = ports.store
        .create(SynthesisInvestigation::from_request(&request)?, context)
        .await?;

    loop {
        let compiled = ports.contexts
            .get_generation(&investigation.current_context_generation_id, context)
            .await?;

        let calculations = ports.codemode
            .execute(SynthesisComputationRequest::from(&investigation, &compiled), context)
            .await?;
        investigation.apply_calculations(calculations)?;

        let proposed_questions = derive_material_questions(&investigation, &compiled)?;
        let dispatchable = proposed_questions.into_iter()
            .filter(|question| investigation.may_dispatch(question, &request.research))
            .take(request.research.max_parallel_jobs)
            .collect::<Vec<_>>();

        if !dispatchable.is_empty() {
            let jobs = ports.research.create_batch(dispatchable, context).await?;
            investigation.attach_jobs(jobs)?;
            ports.store.save(&investigation, context).await?;
            return Ok(investigation.pending_result());
        }

        if investigation.has_completed_research() {
            let generation = ports.contexts
                .enrich_from_completed_research(&investigation, context)
                .await?;
            investigation.advance_context(generation.generation_id)?;
            ports.store.save(&investigation, context).await?;
            continue;
        }

        let draft = ports.model
            .draft_structured_result(&investigation, &compiled, context)
            .await?;
        let verified = verify_synthesis_result(
            draft,
            &compiled,
            &request,
            ports.evidence.as_ref(),
            context,
        ).await?;

        investigation.complete(verified.clone())?;
        ports.store.save(&investigation, context).await?;
        return Ok(verified);
    }
}
~~~

The first release may pause and resume around durable research jobs rather than hold a worker loop open. The state transitions remain the same.

## 6. Synthesis verification

~~~rust
pub async fn verify_synthesis_result(
    mut result: SynthesisResult,
    context: &CompiledContext,
    request: &SynthesisRequest,
    evidence: &dyn EvidenceVerificationPort,
    execution: &ExecutionContext,
) -> Result<SynthesisResult, ApplicationError> {
    validate_json_schema(&result, &request.output_schema)?;

    for claim in &mut result.claims {
        let support = evidence
            .verify_refs(&claim.support, context, execution)
            .await?;
        let contradictions = evidence
            .verify_refs(&claim.contradictions, context, execution)
            .await?;

        if support.is_empty() {
            claim.status = match claim.status {
                EvidenceClass::Claimed | EvidenceClass::Unknown => claim.status,
                _ => EvidenceClass::Unknown,
            };
            claim.confidence = 0.0;
        }
        claim.support = support;
        claim.contradictions = contradictions;
        enforce_claim_classification(claim)?;
    }

    enforce_source_diversity(&result, request)?;
    enforce_primary_source_policy(&result, request)?;
    enforce_context_generation_membership(&result, context)?;
    result.verification = build_verification_report(&result, request)?;
    Ok(result)
}
~~~

## 7. APM process adapter

Proposed file: <code>crates/soma/integrations/src/apm.rs</code>.

~~~rust
#[derive(Clone)]
pub struct ApmPackageManagerPort {
    program: PathBuf,
    runner: Arc<dyn BoundedProcessRunner>,
    cache: Arc<dyn PackageCachePort>,
}

#[async_trait]
impl PackageManagerPort for ApmPackageManagerPort {
    async fn resolve(
        &self,
        request: ResolvePackageRequest,
        context: &ExecutionContext,
    ) -> Result<ResolvedPackage, ApplicationError> {
        let manifest = canonical_regular_file(&request.manifest)?;
        let lock = request.lock.as_deref().map(canonical_regular_file).transpose()?;
        if request.require_lock && lock.is_none() {
            return Err(ApplicationError::invalid_request(
                "apm_lock_required",
                "the agent stack requires apm.lock.yaml",
            ));
        }

        let version = self.run_json(["--version", "--json"], None, context).await?;
        let audit = self.run_json(
            ["audit", "--manifest", path_arg(&manifest), "--json"],
            manifest.parent(),
            context,
        ).await?;
        ensure_apm_audit_passed(&audit)?;

        let manifest_digest = sha256_file(&manifest)?;
        let lock_digest = lock.as_ref().map(sha256_file).transpose()?;
        let cache_key = package_cache_key(&version, &manifest_digest, lock_digest.as_ref())?;
        if let Some(cached) = self.cache.get_verified(&cache_key, context).await? {
            return Ok(cached);
        }

        let inventory = self.run_json(
            build_apm_resolve_args(&manifest, lock.as_deref(), request.target.as_deref()),
            manifest.parent(),
            context,
        ).await?;
        let resolved = map_apm_inventory(
            version,
            manifest,
            manifest_digest,
            lock,
            lock_digest,
            inventory,
        )?;
        self.cache.publish(&cache_key, &resolved, context).await?;
        Ok(resolved)
    }
}

impl ApmPackageManagerPort {
    async fn run_json<I, S>(
        &self,
        args: I,
        cwd: Option<&Path>,
        context: &ExecutionContext,
    ) -> Result<Value, ApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.runner.run(BoundedProcessRequest {
            program: self.program.clone(),
            args: args.into_iter().map(|arg| arg.as_ref().to_owned()).collect(),
            cwd: cwd.map(Path::to_path_buf),
            env_clear: true,
            allowed_env: apm_allowed_env(context),
            timeout: Duration::from_secs(120),
            max_stdout_bytes: 8 * 1024 * 1024,
            max_stderr_bytes: 2 * 1024 * 1024,
            cancellation: context.cancellation.clone(),
        }).await?;
        if !output.status.success() {
            return Err(map_apm_failure(output));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| ApplicationError::invalid_response(
                "apm_invalid_json",
                error.to_string(),
            ))
    }
}
~~~

The exact APM machine-readable arguments must be verified against the pinned or installed CLI. Do not claim an unsupported <code>--json</code> flag. When absent, add an upstream contract or parse documented manifest/lock formats with a clearly scoped adapter.

## 8. Bootstrap wiring

Keep composition in <code>apps/soma/src/bootstrap.rs::runtime_for_components</code>:

~~~rust
pub(crate) fn runtime_for_components(
    service: SomaService,
    provider_registry: ProviderRegistry,
    gateway: GatewayProductState,
    python_environment: Option<Arc<dyn PythonEnvironmentPort>>,
) -> Arc<SomaRuntime> {
    let config = soma_config::Config::load()
        .expect("configuration was validated before runtime construction");
    let paths = soma_config::AgentRuntimePaths::from_default_data_dir()
        .expect("Soma data directory must resolve");

    let mut ports = ApplicationPorts::unavailable()
        .with_gateway(Arc::new(GatewayApplicationPort::new(gateway.clone())))
        .with_codemode(Arc::new(CodeModeApplicationPort::default()));

    if let Some(python_environment) = python_environment {
        ports = ports.with_python_environment(python_environment);
    }

    if config.agent_runtime.enabled {
        let stores = build_agent_runtime_stores(&config, &paths)
            .expect("agent runtime stores must initialize");
        let agent_runtime = build_agent_runtime_ports(
            &config,
            &paths,
            stores,
            gateway,
        ).expect("agent runtime adapters must initialize");
        ports = ports.with_agent_runtime(agent_runtime);
    }

    Arc::new(SomaRuntime::new(SomaApplication::new(
        Arc::new(service),
        Arc::new(provider_registry),
        ports,
    )))
}
~~~

Use normal error propagation in real startup paths rather than <code>expect</code> if the current constructor returns <code>Result</code>. The important rule is that <code>apps/soma</code> constructs adapters and does not implement them.
