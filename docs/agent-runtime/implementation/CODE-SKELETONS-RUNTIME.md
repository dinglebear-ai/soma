---
title: "Agent Runtime Adapter Code Skeletons"
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

# Runtime Adapter Code Skeletons

## 1. Incus instance execution API

Proposed shared-client file: <code>crates/shared/incus-client/src/resources/instance_exec.rs</code>.

The exact wire implementation must follow Incus's operation/websocket protocol and existing client request helpers. The public API should remain transport-neutral within the local Unix-socket client.

~~~rust
use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{IncusClient, Operation, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceExecRequest {
    pub command: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub stdin: Vec<u8>,
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceExecResult {
    pub operation_id: String,
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

impl IncusClient {
    pub async fn exec_instance(
        &self,
        instance: &str,
        request: &InstanceExecRequest,
    ) -> Result<InstanceExecResult> {
        validate_instance_name(instance)?;
        validate_exec_request(request)?;

        let operation: Operation = self
            .post_operation(
                &format!("/1.0/instances/{instance}/exec"),
                &serde_json::json!({
                    "command": request.command,
                    "environment": request.environment,
                    "wait-for-websocket": true,
                    "interactive": false,
                    "record-output": false,
                    "user": 0,
                    "group": 0,
                    "cwd": request.cwd,
                }),
            )
            .await?;

        let channels = self
            .connect_exec_channels(&operation, request.stdin.clone())
            .await?;

        let outcome = tokio::time::timeout(
            request.timeout,
            collect_exec_output(
                channels,
                request.max_stdout_bytes,
                request.max_stderr_bytes,
            ),
        )
        .await
        .map_err(|_| crate::Error::timeout("instance exec timed out"))??;

        let completed = self.wait_operation(&operation.id).await?;
        Ok(InstanceExecResult {
            operation_id: operation.id,
            exit_code: operation_exit_code(&completed)?,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            stdout_truncated: outcome.stdout_truncated,
            stderr_truncated: outcome.stderr_truncated,
        })
    }
}
~~~

Do not invent <code>post_operation</code>, <code>connect_exec_channels</code>, or error constructors if current helpers differ. Add the minimum helpers using existing envelope and socket code.

## 2. Soma Incus runtime adapter

Proposed file: <code>crates/soma/integrations/src/incus_runtime.rs</code>.

~~~rust
#[derive(Clone)]
pub struct IncusAgentRuntimePort {
    client: soma_incus_client::IncusClient,
    paths: soma_config::AgentRuntimePaths,
}

#[async_trait]
impl AgentExecutorPort for IncusAgentRuntimePort {
    async fn provision(
        &self,
        request: ProvisionAgentRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<ProvisionedAgentRuntime, ApplicationError> {
        if request.provider != RuntimeProvider::Incus {
            return Err(unsupported_provider(request.provider));
        }
        if request.remote_endpoint.is_some() {
            return Err(ApplicationError::invalid_request(
                "incus_remote_not_supported",
                "agent runtime currently supports only the local Incus Unix socket",
            ));
        }

        let name = deterministic_instance_name(&request.stack, &request.service, &request.run_id)?;
        let binding = self.lookup_binding(&request.run_id, &name, context).await?;

        if let Some(binding) = binding {
            return self.reconcile(binding, &request, context).await;
        }

        validate_project_and_profiles(&self.client, &request).await?;
        let operation = self.client.create_instance(&render_create_params(&name, &request)?).await?;
        self.client.wait_operation(&operation.id).await?;
        self.record_binding(&request.run_id, &name, &operation.id, context).await?;

        for bootstrap in render_bootstrap_files(&request)? {
            self.client.push_instance_file(
                &name,
                &bootstrap.target,
                &bootstrap.bytes,
                &bootstrap.options,
            ).await?;
        }

        let start = self.client.start_instance(&name).await?;
        self.client.wait_operation(&start.id).await?;
        wait_for_instance_health(&self.client, &name, request.health_timeout).await?;

        Ok(ProvisionedAgentRuntime {
            instance_id: RuntimeInstanceId::new(name.clone())?,
            canonical_ref: CanonicalRef::new(format!(
                "incus://project/{}/instance/{name}", request.project
            ))?,
            operation_refs: vec![operation_ref(operation), operation_ref(start)],
            endpoint: runtime_supervisor_socket(&name),
        })
    }

    async fn execute(
        &self,
        request: ExecuteAgentRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<AgentExecutionResult, ApplicationError> {
        execute_codex_agent(&self.client, request, context).await
    }

    async fn finalize(
        &self,
        request: FinalizeAgentRuntimeRequest,
        context: &ExecutionContext,
    ) -> Result<RuntimeFinalization, ApplicationError> {
        finalize_instance(&self.client, request, context).await
    }
}
~~~

Every call to Incus must publish or enqueue a lifecycle event with run, instance, operation, phase, and trace identities.

## 3. Codex assistant adapter

Proposed file: <code>crates/soma/integrations/src/codex_runtime.rs</code>.

~~~rust
use std::time::Duration;

use async_trait::async_trait;
use codex_app_server_client::{CodexSession, SessionOptions};

pub struct CodexAgentAdapter {
    client_name: String,
    client_version: String,
    default_timeout: Duration,
}

impl CodexAgentAdapter {
    async fn connect(
        &self,
        endpoint: &AgentRuntimeEndpoint,
        request: &ExecuteAgentRuntimeRequest,
    ) -> Result<CodexSession, ApplicationError> {
        let options = SessionOptions::new(&self.client_name, &self.client_version)
            .with_title(format!("Soma run {}", request.run_id))
            .with_call_timeout(request.call_timeout.unwrap_or(self.default_timeout))
            .with_events_capacity(request.event_capacity.max(64));

        match endpoint {
            AgentRuntimeEndpoint::UnixSocket(path) => {
                CodexSession::connect_unix(path, options)
                    .await
                    .map_err(codex_error)
            }
            AgentRuntimeEndpoint::Spawn { command, args } => {
                let options = args.iter().fold(options.with_command(command), |value, arg| {
                    value.with_extra_arg(arg)
                });
                CodexSession::spawn(options).await.map_err(codex_error)
            }
        }
    }
}

pub async fn execute_codex_agent(
    endpoint: AgentRuntimeEndpoint,
    request: ExecuteAgentRuntimeRequest,
    approval: Arc<dyn ApprovalPort>,
    events: Arc<dyn LifecycleEventPort>,
    context: &ExecutionContext,
) -> Result<AgentExecutionResult, ApplicationError> {
    let mut session = CodexAgentAdapter::default()
        .connect(&endpoint, &request)
        .await?;

    let thread = session
        .start_thread_with_model(
            request.workspace.to_string_lossy(),
            request.model.clone(),
        )
        .await
        .map_err(codex_error)?;

    publish_codex_started(&events, &request, &thread, context).await?;

    let prompt = render_bootstrap_prompt(&request.bootstrap)?;
    let result = session
        .run_text_turn_with_model_and_handler(
            thread.thread.id.clone(),
            prompt,
            request.model.clone(),
            |server_request| handle_codex_server_request(
                server_request,
                &request,
                approval.as_ref(),
                events.as_ref(),
                context,
            ),
        )
        .await
        .map_err(codex_error)?;

    publish_codex_completed(&events, &request, &result, context).await?;
    validate_agent_output(&result.agent_message, &request.output_contract)?;

    Ok(AgentExecutionResult {
        message: result.agent_message,
        diff: result.latest_diff,
        errors: result.errors.into_iter().map(map_turn_error).collect(),
        thread_id: thread.thread.id,
        turn_id: result.turn_id,
        output_bytes: result.output_bytes,
    })
}
~~~

Reconcile exact generated Codex types and helper signatures with current main. Do not leak those types into Soma domain DTOs.

## 4. LABBY loadout adapter

Proposed file: <code>crates/soma/integrations/src/labby_loadout.rs</code>.

~~~rust
#[derive(Clone)]
pub struct LabbyLoadoutAdapter {
    client: Arc<dyn LabbyCatalogClient>,
    signer: Arc<dyn RunCapabilitySigner>,
}

#[async_trait]
impl LoadoutPort for LabbyLoadoutAdapter {
    async fn resolve(
        &self,
        request: ResolveLoadoutRequest,
        context: &ExecutionContext,
    ) -> Result<LoadoutResolution, ApplicationError> {
        if request.loadout.mode == LoadoutMode::Physical {
            return Err(ApplicationError::invalid_request(
                "physical_loadout_not_supported",
                "physical LABBY loadouts are reserved but not implemented",
            ));
        }

        let catalog = self.client.snapshot(context).await?;
        let selected = resolve_catalog_selection(
            &catalog,
            &request.loadout,
            &request.requirements,
        )?;
        let effective = intersect_capabilities([
            request.package_capabilities,
            request.stack_capabilities,
            request.context_capabilities,
            request.snippet_capabilities,
            selected.capabilities,
            request.runtime_capabilities,
            context_authorization_capabilities(context),
        ])?;

        enforce_required_capabilities(&request.requirements, &effective)?;
        let claims = RunCapabilityClaims {
            run_id: request.run_id.clone(),
            agent_id: request.agent_id.clone(),
            subject: context.principal.subject().map(ToOwned::to_owned),
            catalog_generation: catalog.generation.clone(),
            capabilities: effective.clone(),
            expires_at: request.expires_at,
        };
        let token_ref = self.signer.issue(claims, context).await?;

        Ok(LoadoutResolution {
            generation: 1,
            catalog_generation: catalog.generation,
            capabilities: effective,
            token_ref,
            missing: selected.missing,
            denied: selected.denied,
            unhealthy: selected.unhealthy,
            warnings: selected.warnings,
        })
    }

    async fn release(
        &self,
        run_id: &RunId,
        context: &ExecutionContext,
    ) -> Result<(), ApplicationError> {
        self.signer.revoke(run_id, context).await
    }
}
~~~

The real client must use a supported LABBY API/MCP contract. Do not reach into LABBY's private storage files.

## 5. Deterministic progressive disclosure evaluator

Proposed file: <code>crates/soma/application/src/agent_runtime/disclosure/policy.rs</code>.

~~~rust
pub fn decide_disclosure(
    request: DisclosureRequest,
    policy: &EffectiveDisclosurePolicy,
    context: &CompiledContext,
    authorization: &AuthorizationContext,
) -> Result<DisclosureDecision, ApplicationError> {
    if request.context_generation_id != context.generation_id {
        return Err(ApplicationError::invalid_request(
            "disclosure_context_mismatch",
            "disclosure request targets a different context generation",
        ));
    }
    if request.requested_level > policy.max_level {
        return Ok(DisclosureDecision::denied(
            request,
            "disclosure_level_denied",
        ));
    }

    let authorized = context.items.iter()
        .filter(|item| request.selectors.iter().any(|selector| selector.matches(item)))
        .filter(|item| authorization.may_read(&item.canonical_ref, item.sensitivity))
        .collect::<Vec<_>>();

    let protected_count = request.estimated_selector_count.saturating_sub(authorized.len());
    let selected = prefer_derived_before_raw(
        authorized,
        request.raw_evidence,
        request.requested_level,
    );
    let bounded = apply_disclosure_budget(selected, &request.budget)?;

    let requires_approval = bounded.items.iter().any(|item| {
        policy.requires_approval(item.sensitivity, request.requested_level)
    });
    if requires_approval {
        return Ok(DisclosureDecision::approval_required(
            request,
            bounded.summary_without_protected_counts(),
        ));
    }

    let status = if bounded.truncated || protected_count > 0 {
        DisclosureDecisionStatus::Narrowed
    } else {
        DisclosureDecisionStatus::Allowed
    };

    Ok(DisclosureDecision {
        id: new_disclosure_decision_id(),
        request_id: request.id,
        status,
        granted_level: Some(request.requested_level),
        reason_codes: bounded.reason_codes,
        selected_item_ids: bounded.items.into_iter().map(|item| item.id).collect(),
        omitted_item_ids: Vec::new(),
        decided_at: now_rfc3339(),
    })
}
~~~

Do not disclose protected item IDs in <code>omitted_item_ids</code>. That field is for authorized items omitted because of budget or representation.

## 6. Transactional lifecycle outbox

Proposed runtime store method:

~~~rust
pub fn transition_with_event(
    tx: &rusqlite::Transaction<'_>,
    request: &TransitionAgentRunRequest,
    event: &LifecycleEvent,
) -> Result<AgentRun, StoreError> {
    let changed = tx.execute(
        r#"
        UPDATE agent_runs
           SET state = ?1,
               state_version = state_version + 1,
               updated_at = ?2
         WHERE id = ?3
           AND state = ?4
           AND state_version = ?5
           AND terminal_at IS NULL
        "#,
        rusqlite::params![
            request.next_state.as_str(),
            request.updated_at,
            request.run_id.as_str(),
            request.expected_state.as_str(),
            request.expected_state_version,
        ],
    )?;
    if changed != 1 {
        return Err(StoreError::Conflict("agent run state changed".into()));
    }

    tx.execute(
        r#"
        INSERT INTO lifecycle_outbox
            (event_id, run_id, event_kind, event_json, created_at, attempts)
        VALUES (?1, ?2, ?3, ?4, ?5, 0)
        "#,
        rusqlite::params![
            event.id.as_str(),
            request.run_id.as_str(),
            event.kind,
            serde_json::to_vec(event)?,
            event.ingestion_time,
        ],
    )?;

    load_agent_run(tx, &request.run_id)
}
~~~

The publisher leases outbox rows, sends idempotency key <code>event_id</code> through the Cortex adapter, then marks delivery. It never deletes undelivered critical state events because a process restarted.
