---
title: "Implementation Phases AR-11 through AR-15"
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

# Phases AR-11 through AR-15

## AR-11: Lifecycle event outbox and Cortex projection

### Donor anchors

Use Cortex behavior from:

- <code>src/db/models.rs</code> for canonical log/session records;
- <code>src/db/heartbeat.rs</code> for heartbeat windows;
- <code>src/db/graph.rs</code> for entities, relationships, trust, reasons, evidence, and multi-hop traversal;
- <code>src/agent/ai_transcript.rs</code> and <code>shell_history.rs</code>;
- <code>src/agent/journald.rs</code> and <code>syslog_file.rs</code>;
- <code>src/docker_ingest/</code>, <code>src/otlp/</code>, <code>src/inventory/</code>, and receiver/ingest code.

### Instructions

1. Add <code>LifecycleEvent</code> to the Soma domain and schema bundle.
2. Add a lifecycle outbox table in the same control-store transaction as run state changes.
3. Implement a bounded publisher adapter that sends events through a Cortex-compatible ingestion boundary with idempotency keys.
4. Project run, context, disclosure, snippet, tool, command, claim, research, artifact, Incus, and verification entities and relationships into the evidence graph.
5. Preserve source class: an agent claim remains a claim until canonical evidence resolves it.
6. Correlate run IDs with trace/span IDs, Code Mode execution IDs, Incus operation IDs, Codex thread/turn IDs, and canonical repository/host/service/device IDs.
7. Add retention tiers for raw 30-second heartbeats and longer aggregates while retaining full-resolution incident windows.

### Critical versus advisory delivery

Run-state transitions require durable outbox insertion. Temporary Cortex delivery failure may degrade telemetry but cannot lose the event intent. Non-critical high-volume samples may use bounded loss policy already established by Cortex ingestion.

### Tests

- duplicate event delivery is idempotent;
- state transition and event intent commit atomically;
- secret redaction covers prompts, tool params, auth headers, cookies, tokens, and environment;
- clock skew does not create a false total order;
- graph claims retain canonical event evidence;
- query reconstructs what the agent knew before a claim or edit.

## AR-12: Run-scoped context-aware Code Mode

### Current anchor

<code>CodeModeApplicationPort</code> currently owns <code>CodeModeConfig</code>, checks <code>enabled</code>, and calls <code>execute_inline</code>. The shared host already provides tool listing/calls, snippet resolution, semantic search, step replay, artifacts, state, redaction, and budgets.

### Required changes

1. Add run-aware configuration and host creation to the adapter.
2. Thread caller input into resolved snippets. The current adapter explicitly documents that input is not yet passed through the runner protocol; fix that shared protocol once and use it for both application and provider Code Mode paths.
3. Construct the host from effective LABBY tools plus Soma context/disclosure/research actions.
4. Keep global tools absent from the scoped host.
5. Attach <code>CodeModeCaller</code>, <code>ToolScope</code>, run/context IDs, execution ID, step ordinal, trace context, and policy generation to every call.
6. Publish call, snippet, step, artifact, and error lifecycle events.

### Initial Soma context actions

~~~text
context.catalog
context.entity.resolve
context.entity.neighborhood
context.timeline
context.evidence
context.compare
context.materialize
context.disclosure.request
research.create
research.status
artifact.write
~~~

These actions call application ports. They do not open SQLite, Qdrant, files, or Cortex/Axon stores directly from the Code Mode crate.

### Example snippet acceptance

<code>examples/trace-service-failure.snippet.md</code> must:

- resolve typed <code>service</code> and <code>since</code> inputs;
- see only the loadout's allowed tools;
- request evidence-level disclosure;
- run timeline and graph queries;
- create bounded dependent research only when needed;
- write the incident timeline artifact;
- return schema-valid structured output;
- emit a complete call trace with no secrets.

## AR-13: Dependent Axon research and structured synthesis

### Donor anchors

- <code>axon-retrieval/src/query.rs</code>, <code>plan.rs</code>, <code>context.rs</code>, <code>ask_context.rs</code>, and <code>service.rs</code>;
- <code>axon-services/src/query/synthesis/</code>;
- <code>axon-jobs/src/unified/</code> and workers;
- <code>axon-graph/src/candidate.rs</code> and <code>evidence.rs</code>;
- <code>axon-llm/src/runtime/codex_app_server/</code>;
- <code>axon-observe</code>.

### Instructions

1. Add research-question and synthesis-investigation aggregates to the application/store.
2. Normalize a question and derive a digest from question, constraints, evidence IDs, source policy, and depth for deduplication.
3. Persist <code>derivedFrom</code> edges before dispatch.
4. Dispatch through the durable job engine with max depth one, a small parallel-job bound, deadline, source policy, and output schema.
5. Wrap existing Axon retrieval and synthesis DTOs. Do not invent a second retrieval pipeline.
6. On completion, hydrate canonical sources and citations, create graph candidates/evidence, and publish a child compiled-context generation.
7. Resume the synthesis investigation against the child generation.
8. Validate structured claims, conflicts, open questions, actions, budgets, and terminal status against <code>synthesis-result.schema.json</code>.
9. Generate Markdown after structured validation.

### Stopping conditions v1

Stop when required outputs are evidence-supported, one dependent generation has completed, no additional allowed job is material, approval is required, or budgets expire. Budget exhaustion and insufficient evidence are honest terminal statuses.

### Verification

- unsupported claim becomes <code>unknown</code> or is rejected;
- contradicted hypotheses remain in output;
- primary-source and source-diversity requirements are enforced;
- original context is immutable;
- child context records research job and additions;
- narrative cites the same canonical evidence as the structured result;
- rerun against pinned inputs preserves non-model planning and calculation results.

## AR-14: APM package adapter

### Pinned behavior

At the baseline commit, APM owns <code>apm.yml</code>, <code>apm.lock.yaml</code>, instructions, skills, prompts, agents, hooks, plugins, MCP dependencies, install, compile, audit, policy, integrity, drift, pack, and distribution. APM policy controls installation; Soma controls execution.

### Adapter

Add <code>PackageManagerPort</code> to the application and a bounded process adapter in integrations/runtime. Use the existing shared process/operations safety patterns for executable resolution, env clearing, timeout, output caps, cancellation, and error mapping.

Required operations:

- version and capability probe;
- manifest validation;
- lock verification;
- policy and integrity audit;
- install or verify cache;
- resolved primitive inventory;
- target compilation when requested.

Prefer machine-readable APM output. If one operation lacks a stable machine format, add an upstream APM contract or parse documented manifest/lock files, not colorized human output.

### Resolution receipt

Record:

- APM version;
- manifest and lock canonical paths and SHA-256;
- package source, version, integrity, dependencies, and cache key;
- selected prompt/skill/agent/plugin/MCP primitive identities and hashes;
- audit and policy result;
- compiled target;
- immutable package-root receipt.

Mount the resolved package read-only at <code>/soma/package</code>. Package hooks remain inert. MCP dependencies require separate LABBY installation and loadout exposure.

### Failure tests

- missing required lock;
- manifest/lock drift;
- failed audit or policy;
- integrity mismatch;
- missing selected primitive;
- timeout/cancellation;
- unexpected output size;
- cache collision;
- package attempts to broaden stack capability.

All fail before Incus provisioning.

## AR-15: Surfaces and read-only vertical slice

### Surface rule

CLI, REST/OpenAPI, MCP, and Aurora web call the same <code>SomaApplication</code> methods and receive identical IDs, error codes, authorization decisions, and progress semantics.

### CLI families

Add thin commands or action mappings:

~~~text
soma stack validate|resolve|run
soma run get|list|cancel|approve|retry|artifacts
soma context validate|compile|get|compare|materialize
soma snippets list|get|execute|promote|remove
soma loadout validate|resolve|get|refresh
soma disclosure list|get|request
soma synthesis get
~~~

Follow the current compact-action architecture where appropriate. Do not duplicate orchestration inside CLI handlers.

### REST/OpenAPI families

~~~text
/api/v1/agent-stacks
/api/v1/agent-runs
/api/v1/contexts
/api/v1/snippets
/api/v1/loadouts
/api/v1/disclosures
/api/v1/synthesis
~~~

Long-running operations return job/run resources. Generate OpenAPI through the existing contract path.

### MCP actions

Expose compact actions such as:

~~~text
stack.validate
stack.run
run.get
run.cancel
context.compile
context.get
context.materialize
snippet.execute
disclosure.request
~~~

Code Mode accesses the same authorized use cases without hundreds of top-level schemas.

### Aurora web

Add run list/detail, state timeline, compiled-context explorer, disclosure history, scoped tool catalog, snippet execution, claims/evidence, artifacts, Incus state, and cleanup status. The web app consumes REST/client APIs only.

### Full acceptance run

Run <code>examples/soma.stack.yaml</code> with:

- local Soma built from the implementation branch;
- live LABBY catalog containing required read-only upstreams;
- Axon and Cortex context sources healthy;
- local Incus test project and profile;
- pinned APM package and lock;
- read-only repository and docs mounts.

Verify in order:

1. stack, context, loadout, and snippet validation;
2. APM audit/lock/package receipt;
3. context compile and deterministic generated manifest;
4. server-side loadout catalog pin;
5. Incus create/config/start/health and target identity;
6. supervisor and Codex session initialization;
7. bootstrap disclosure only;
8. context requests and receipts;
9. snippet execution and artifact;
10. one dependent Axon job and child context;
11. structured synthesis and Markdown briefing;
12. output-schema and citation verification;
13. Cortex lifecycle timeline and graph relationships;
14. run manifest publication;
15. instance stop/delete or failure snapshot according to policy.

### Release gate

AR-15 is verified only when a fresh environment can reproduce the run from the stack, context, loadout, APM manifest/lock, repository revision, and declared external services. Record versions, commits, tool counts, context counts, Incus identity, runtime status, artifacts, and cleanup result in <code>PROGRESS.md</code>.

Do not start resident assistants, multi-service orchestration, physical gateways, mutation workflows, custom Incus images, or remote Incus until this gate passes.
