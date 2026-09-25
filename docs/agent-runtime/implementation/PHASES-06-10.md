---
title: "Implementation Phases AR-06 through AR-10"
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

# Phases AR-06 through AR-10

## AR-06: Context materialization

### Files

- add application materialization request/response DTOs and port;
- add runtime materializers under <code>crates/soma/runtime/src/agent_runtime/materialization*</code>;
- reuse <code>soma-codemode</code> artifact store and path-safety helpers;
- project MCP resources through the existing Soma MCP application boundary.

### Formats

Implement in this order:

1. generated compiled-context manifest;
2. Markdown briefing;
3. bounded filesystem tree;
4. graph subgraph JSON;
5. event JSONL;
6. Code Mode dataset handle;
7. MCP resource projection.

The filesystem tree must be read-only, rooted beneath the run directory, and carry a path-to-canonical-reference index. Large raw logs, traces, metrics, and transcripts remain query handles unless an explicit bounded materialization is approved.

The first global-docs mode is a read-only mount at <code>/soma/docs</code>. Do not build FUSE yet. Repository-local symlinks may point only inside the approved docs root and must reject escapes.

### Publication

Use atomic temp/write/fsync/rename publication patterns already present in gateway config, setup <code>.env</code>, provider state, Tauri persistence, and Code Mode artifacts. Every receipt contains context generation, kind, URI/path, digest, size, content type, item count, policy decision, and creation time.

### Tests

- deterministic path mapping;
- static and dynamic size caps;
- symlink escape rejection;
- context and package roots reject writes;
- raw materialization requires disclosure receipt;
- deletion of a projection does not delete canonical evidence;
- portable pack contains all referenced required evidence.

## AR-07: LABBY loadout resolution

### Current donor anchors

- LABBY gateway config mutation;
- <code>gateway/code_mode/code_mode_host.rs</code> for scoped catalog and journaling;
- <code>gateway/manager/virtual_servers.rs</code> for surface policy;
- gateway runtime views, catalog events, OAuth subject behavior, and usage records.

### Application boundary

Add <code>GatewayLoadoutPort</code> with:

~~~rust
async fn resolve(
    &self,
    request: ResolveLoadoutRequest,
    context: &ExecutionContext,
) -> Result<LoadoutResolution, ApplicationError>;

async fn refresh(
    &self,
    request: RefreshLoadoutRequest,
    context: &ExecutionContext,
) -> Result<LoadoutResolution, ApplicationError>;

async fn release(
    &self,
    run_id: &RunId,
    context: &ExecutionContext,
) -> Result<(), ApplicationError>;
~~~

### Adapter instructions

1. Load and validate <code>LabbyLoadout</code>.
2. Query LABBY's live upstream/tool/virtual-server catalog through its supported API or MCP boundary.
3. Resolve allow/deny selections and required entries.
4. Intersect package, stack, context, snippet, LABBY, runtime, and caller capability sets.
5. Pin the current catalog generation.
6. Create a run-bound logical policy or token enforced server-side by LABBY's Code Mode host.
7. Record missing, denied, unhealthy, quarantined, and optional entries.
8. Publish the resolution and lifecycle events.

Do not edit or rename existing LABBY upstream definitions. A loadout is a scoped view. Physical mode may return a stable <code>physical_loadout_not_supported</code> error in the first slice.

### Security tests

- global catalog is not returned to a scoped run;
- wildcard denial overrides allow;
- mutation class cannot be lowered by a snippet;
- expired run policy cannot call a tool;
- credential subject and caller identity are bound;
- missing required tool fails before Incus provisioning;
- OAuth and tool parameters remain redacted;
- catalog refresh creates a new generation and reports differences.

## AR-08: Incus workload operations

### Existing APIs to reuse

The shared client already supports local Unix-socket transport, instance list/get/create/update/patch/delete/start/stop/restart/pause, snapshots, operations, projects, profiles, storage, networks, certificates, and event subscription.

### Additions

Add only missing workload APIs under <code>crates/shared/incus-client/src/resources</code>:

- <code>instance_exec.rs</code>;
- <code>instance_files.rs</code>;
- <code>instance_state.rs</code>;
- operation wait helpers in the existing operations module.

Proposed API shapes:

~~~rust
pub struct InstanceExecRequest {
    pub command: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub stdin: Option<Vec<u8>>,
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

pub struct InstanceExecResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub operation_id: String,
    pub truncated: bool,
}

impl IncusClient {
    pub async fn exec_instance(
        &self,
        name: &str,
        request: &InstanceExecRequest,
    ) -> Result<InstanceExecResult>;

    pub async fn push_instance_file(
        &self,
        name: &str,
        path: &str,
        bytes: &[u8],
        options: &PushFileOptions,
    ) -> Result<()>;

    pub async fn pull_instance_file(
        &self,
        name: &str,
        path: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>>;
}
~~~

Use Incus API endpoints and websocket/operation semantics directly. Apply existing request, envelope, ETag, error, and test conventions. Do not shell out to <code>incus</code>.

### Provisioning adapter

The Soma runtime adapter must:

1. validate local socket and project;
2. resolve image, profiles, networks, storage, and mounts;
3. create a deterministic instance name;
4. reconcile an existing incomplete instance only when bound to the same run;
5. attach limits and mounts;
6. transfer bootstrap files;
7. subscribe to events;
8. start and wait for health;
9. return canonical instance and operation references.

### Integration test

Against a dedicated test project:

~~~text
create temporary instance
-> wait operation
-> start
-> push bootstrap file
-> exec hostname and id
-> inspect state/resources
-> pull output file
-> stop
-> delete
~~~

Verify target identity before using the instance in any agent run.

## AR-09: Codex assistant adapter and supervisor

### Existing API

Use <code>SessionOptions</code>, <code>CodexSession::spawn</code> or <code>connect_unix</code>, <code>start_thread</code>, <code>run_text_turn*</code>, approval handlers, event collection, diffs, errors, and terminal status from <code>crates/shared/codex-app-server-client</code>.

### Supervisor

Add a small binary or mode, proposed name <code>soma-agent-supervisor</code>, in a new app/shared boundary only if independent reuse is proven. Its responsibilities:

- parse one bootstrap JSON file;
- verify run, service, package, context, and loadout IDs;
- start Codex app-server with explicit command/config;
- expose a controlled Unix socket or stdio bridge;
- emit health and lifecycle events;
- forward stdout/stderr with caps;
- collect transcript and terminal receipts;
- enforce cancellation and timeout;
- write outputs under <code>/run/artifacts</code>.

It must not implement orchestration, policy, package resolution, context compilation, or autonomous retry.

### Host adapter sequence

~~~text
provisioned Incus instance
-> transfer supervisor/bootstrap/package receipts
-> start supervisor through instance exec
-> wait health/socket
-> connect CodexSession
-> initialize with explicit capabilities
-> start thread with /workspace cwd
-> send bootstrap prompt
-> handle approvals through Soma policy
-> collect events and result
-> validate output schema
-> close session and supervisor
~~~

### Approval behavior

Map Codex approval requests to the run's effective mutation class. Read-only runs deny repository/runtime/infrastructure mutation. Never auto-approve because the agent is isolated.

### Tests

- spawn/connect and initialize;
- explicit command and config propagation;
- bounded event capacity and call timeout;
- approval allow/deny/timeout;
- cancellation during turn;
- terminal status and error preservation;
- output byte caps;
- no Codex-specific protocol types leak into Soma domain types.

## AR-10: Progressive disclosure controller

### Modules

- domain disclosure request/decision/receipt;
- application policy evaluator and use cases;
- durable store and lifecycle events;
- runtime materialization/read adapter;
- Code Mode context action adapter;
- surface projections.

### Decision algorithm v1

Use deterministic policy rules:

~~~text
validate request and parent run/context
-> verify requested level <= stack/context maximum
-> authorize source classes and selectors
-> prefer existing summary/evidence bundle over raw
-> apply sensitivity, relevance, freshness, and source policy
-> enforce item/byte/token budgets
-> choose allowed, narrowed, denied, or approval-required
-> publish decision and receipt
~~~

A model may propose selectors but cannot authorize them.

### Required distinctions

- eligible context;
- mounted/materialized context;
- disclosed context;
- cited evidence;
- visible tool/skill catalog;
- invoked tools/skills.

A path mounted inside the instance is not disclosed until a recorded read, prompt inclusion, or tool result exposes it to the agent.

### Bootstrap

Construct bootstrap content only from the stack declaration:

- identity and task;
- acceptance criteria;
- repository summary and revision;
- policies and risk class;
- domain catalogs;
- scoped tool/snippet/skill catalogs;
- expected outputs and verification.

### Tests

- bootstrap contains no raw logs or transcripts;
- summary is chosen before raw evidence;
- raw/restricted source needs policy or approval;
- protected entity existence does not leak through denial;
- budget narrowing records omitted selectors;
- decision expiry blocks later reads;
- replay can use original receipts exactly;
- every decision is visible in Cortex run history.
