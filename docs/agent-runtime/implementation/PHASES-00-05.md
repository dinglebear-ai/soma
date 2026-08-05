---
title: "Implementation Phases AR-00 through AR-05"
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

# Phases AR-00 through AR-05

## AR-00: Contracts, schemas, fixtures, and drift checks

### Current anchors

- <code>docs/contracts/provider-manifest.schema.json</code>;
- <code>scripts/generate-docs.py</code>;
- <code>xtask/src/generated_surfaces.rs</code> and generated checks;
- <code>apps/soma/tests/architecture_boundaries.rs</code>.

### Instructions

1. Copy or generate the schemas under <code>docs/generated/agent-runtime/</code> from the source files in this package.
2. Register every schema and example in the existing generated-surface manifest and stale-claim checks.
3. Add fixture validation that parses YAML into JSON-compatible values and validates with JSON Schema draft 2020-12 using a local registry.
4. Add a donor lock file recording full commits from <code>BASELINES.md</code> and hashes of every transplanted donor file per PR.
5. Add CI failure for schema/example drift and invalid Markdown snippet frontmatter/code extraction.

### Tests

- each schema parses;
- each example validates;
- broken local <code>$ref</code> fails;
- unknown fields fail on security-sensitive manifests;
- snippet body code is injected into the resolved definition before schema validation;
- generated check detects edited output.

## AR-01: Runtime path and configuration normalization

### Files

- edit <code>crates/soma/config/src/config.rs</code> and <code>lib.rs</code>;
- add <code>crates/soma/config/src/agent_runtime_paths.rs</code> and sibling tests;
- edit <code>apps/soma/src/bootstrap.rs</code> provider-path defaults;
- update setup and doctor checks after paths are authoritative.

### Instructions

Implement one value object created from <code>default_data_dir()</code>:

~~~rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRuntimePaths {
    root: PathBuf,
}

impl AgentRuntimePaths {
    pub fn from_default_data_dir() -> anyhow::Result<Self> {
        Self::new(default_data_dir()?)
    }

    pub fn new(root: PathBuf) -> anyhow::Result<Self> {
        if !root.is_absolute() {
            anyhow::bail!("Soma data root must be absolute");
        }
        Ok(Self { root })
    }

    pub fn stacks(&self) -> PathBuf { self.root.join("stacks") }
    pub fn contexts(&self) -> PathBuf { self.root.join("contexts") }
    pub fn snippets(&self) -> PathBuf { self.root.join("snippets") }
    pub fn loadouts(&self) -> PathBuf { self.root.join("loadouts") }
    pub fn packages(&self) -> PathBuf { self.root.join("packages") }
    pub fn runs(&self) -> PathBuf { self.root.join("runs") }
    pub fn logs(&self) -> PathBuf { self.root.join("logs") }
    pub fn cache(&self) -> PathBuf { self.root.join("cache") }
    pub fn providers(&self) -> PathBuf { self.root.join("providers") }
}
~~~

Then:

- change <code>Config::load()</code> to use <code>default_data_dir()?.join("config.toml")</code> before <code>./config.toml</code>;
- change the provider default in <code>apps/soma/src/bootstrap.rs</code> from <code>PathBuf::from("providers")</code> to <code>AgentRuntimePaths::from_default_data_dir()?.providers()</code>;
- preserve explicit <code>SOMA_PROVIDER_DIR</code> precedence;
- create only required directories during setup, using restrictive permissions;
- reject symlinked manifests, loadouts, secret files, and sensitive roots using existing path-safety patterns;
- do not migrate auth or gateway files in this PR.

### Configuration sections

Add typed sections to <code>Config</code> only as their phases land:

~~~toml
[agent_runtime]
enabled = false
worker_concurrency = 1

[agent_runtime.incus]
project = "default"
socket = "/var/lib/incus/unix.socket"

[agent_runtime.apm]
program = "apm"
require_lock = true

[agent_runtime.retention]
run_metadata_secs = 2592000
artifacts_secs = 604800
~~~

Avoid speculative fields that no adapter consumes yet.

## AR-02: Domain types, application ports, and unavailable wiring

### Domain files

Add the module family documented under <code>types/</code>. Export it from <code>crates/soma/domain/src/lib.rs</code>. Follow the current missing-docs policy and no-<code>mod.rs</code> convention.

### Application files

Add <code>crates/soma/application/src/agent_runtime.rs</code> and sibling modules. Extend <code>ApplicationPorts</code> exactly where current gateway, Code Mode, OpenAPI, and Python ports live:

~~~rust
pub struct ApplicationPorts {
    pub gateway: Arc<dyn GatewayPort>,
    pub codemode: Arc<dyn CodeModePort>,
    pub openapi: Arc<dyn OpenApiPort>,
    pub python_environment: Arc<dyn PythonEnvironmentPort>,
    pub agent_runtime: AgentRuntimePorts,
}

impl ApplicationPorts {
    pub fn unavailable() -> Self {
        let port = Arc::new(UnavailableEnginePort);
        Self {
            gateway: port.clone(),
            codemode: port.clone(),
            openapi: port.clone(),
            python_environment: port,
            agent_runtime: AgentRuntimePorts::unavailable(),
        }
    }

    pub fn with_agent_runtime(mut self, ports: AgentRuntimePorts) -> Self {
        self.agent_runtime = ports;
        self
    }
}
~~~

Add one method per application use case to <code>SomaApplication</code>. Do not dispatch agent-runtime operations through provider actions. Preserve <code>ExecutionContext</code> on every call.

In <code>apps/soma/src/bootstrap.rs::runtime_for_components</code>, keep the new ports unavailable until concrete phases land. That proves surfaces and architecture can compile without enabling the runtime.

## AR-03: Durable run control and lifecycle worker

### Donor

Transplant product-neutral behavior from Axon <code>crates/axon-jobs/src/unified</code>, <code>workers</code>, <code>state_machine.rs</code>, cancellation, watchdog, artifacts, pagination, and retention at commit in <code>BASELINES.md</code>.

### Instructions

1. Land or reuse the context-v1 shared jobs crate rather than copy logic into <code>crates/soma/application</code>.
2. Add an <code>agent-run</code> job kind whose payload is a resolved-stack reference and whose progress contains orchestration phase and external bindings.
3. Add Soma control-store tables for resolved stacks, runs, attempts, transitions, durable steps, approvals, external resources, output receipts, and lifecycle outbox.
4. Commit state transition and outbox event intent in one transaction.
5. Give every side-effectful step a stable name and recorded result.
6. On recovery, replay recorded results or reconcile external state before executing again.
7. Keep terminal states immutable and cleanup state separate.

### Minimum durable steps

~~~text
package.resolve
context.compile
loadout.resolve
incus.provision
runtime.bootstrap
agent.execute
outputs.verify
run.finalize
runtime.cleanup
~~~

### Tests

Simulate process death after every step. Prove lease expiry, watchdog recovery, duplicate worker contention, cancellation, retry child creation, terminal immutability, and outbox redelivery.

## AR-04: Shared Code Mode snippet store

### Donor

Port from LABBY:

- <code>crates/labby-codemode/src/snippet/store.rs</code>;
- <code>crates/labby/src/dispatch/snippets/catalog.rs</code>;
- <code>dispatch.rs</code> and promotion behavior.

### Instructions

1. Move filesystem store, Markdown parser, frontmatter parser, JS extraction, input validation, create/promote/list/resolve/remove, and collision detection into <code>crates/shared/codemode/src/snippet/</code>.
2. Preserve current Soma limits: 20 KiB code, 32 resolves/run, depth and recursion checks, 256 KiB resolved bytes, path safety, call budgets, and artifact quotas.
3. Extend <code>SnippetInfo</code> with version, risk, skills, context, tools, output schema, source ref, and digest.
4. Keep compatibility constructors for current inline/file snippets.
5. Add a Soma product catalog that merges stack-local, APM, user, and built-in sources using the contract precedence.
6. Resolve requirements before invoking <code>execute_inline</code>.

### Tests

Run the LABBY donor fixtures plus Soma tests for recursive snippets, requirement cycles, denied tool classes, invalid version ranges, unsafe symlinks, collision rules, promotion provenance, and schema-validated output.

## AR-05: Context manifest validation, compilation, and immutable store

### Foundations

Use context-v1 application use cases and contracts. Reuse Axon retrieval DTOs and planning, source generations, canonical citations, graph evidence, and Cortex observation/graph records.

### Compiler steps

1. Parse YAML or JSON.
2. Validate schema.
3. Resolve imports and named views.
4. Resolve roots and repository revision.
5. Enforce dirty-state policy.
6. Resolve source availability and freshness.
7. Build an authorized plan across SQL, FTS, vector, graph, memory, source, and observation lanes.
8. Execute and fuse lanes.
9. Hydrate canonical evidence.
10. Classify trust, freshness, sensitivity, conflicts, exclusions, and unknowns.
11. Enforce deterministic budgets and truncation.
12. Publish immutable compiled-context metadata and selected-item index.

### Required application ports

- context manifest source/store;
- context query planner/executor;
- canonical evidence hydrator;
- graph projection query;
- compiled-context store;
- source-freshness resolver.

The application layer owns compilation policy. Concrete Axon/Cortex storage adapters remain runtime/integration code.

### Acceptance

- the example context compiles against fixtures;
- required-source loss fails before provisioning;
- optional-source loss is a warning;
- authorization removes records before fusion and counts;
- conflicting evidence survives;
- repeat compile against pinned snapshots is deterministic;
- enrichment publishes a child generation without changing the parent.
