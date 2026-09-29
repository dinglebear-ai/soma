---
title: "Agent Runtime Test Matrix"
created: 2026-08-05
updated: 2026-08-05
doc_type: "test-plan"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Test Matrix

## Universal pull-request gate

Every implementation pull request runs:

~~~bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p <changed-crate>
cargo test -p soma --test architecture_boundaries
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --no-deps
cargo xtask check-architecture
cargo xtask check-docs
cargo xtask check-stale-claims
cargo xtask check-schema-docs
cargo xtask check-test-siblings
git diff --check
~~~

Run <code>cargo xtask run-ascii-check --check</code> when the changed paths are covered by the repository's ASCII contract. Run <code>cargo xtask check-coupled-files</code> with the PR base/head arguments required by its current help when changing generated or mirrored surfaces.

Do not substitute a workspace-wide green build for focused feature tests. Do not mark a runtime phase verified without one safe live invocation.

## AR-00: Contracts and schemas

### Unit/static

- all JSON files parse;
- schemas pass draft 2020-12 meta-schema validation;
- all local <code>$ref</code> targets resolve without network access;
- all YAML and JSON examples validate;
- Markdown snippet authoring extracts exactly one JavaScript block;
- unknown security-sensitive fields fail;
- invalid IDs, digests, timestamps, enums, and size caps fail;
- generated schema copies match source schemas;
- file index and checksums match package contents.

### Commands

~~~bash
python3 scripts/check-agent-runtime-docs.py
cargo xtask check-docs
cargo xtask check-schema-docs
cargo xtask check-stale-claims
~~~

The planned checker should replace the temporary validation script once the package lands.

## AR-01: Paths and config

### Unit

- bare-metal path is <code>$HOME/.soma</code>;
- <code>SOMA_HOME</code> override wins;
- container path resolves to <code>/data</code>;
- config loads from <code>default_data_dir()/config.toml</code> before CWD;
- explicit provider directory wins;
- default provider directory is <code>&lt;data-root&gt;/providers</code>;
- runtime directory creation is idempotent;
- non-absolute roots fail;
- symlink roots and sensitive files fail;
- Unix modes are restrictive;
- missing HOME has the existing documented failure or container fallback.

### Commands

~~~bash
cargo test -p soma-config agent_runtime_paths
cargo test -p soma-config config
cargo test -p soma-cli setup
cargo test -p soma-cli doctor
cargo test -p soma bootstrap
~~~

## AR-02: Domain and application boundaries

### Unit

- ID validation and serialization;
- state transition matrix;
- capability intersection and mutation ordering;
- context, disclosure, snippet, stack, run, and synthesis DTO round trips;
- schema fixtures deserialize into proposed Rust DTOs;
- unavailable port bundle returns <code>engine_unavailable</code>;
- <code>with_agent_runtime</code> preserves existing ports;
- <code>SomaApplication</code> passes <code>ExecutionContext</code> unchanged;
- no product/runtime dependencies enter <code>soma-domain</code>.

### Commands

~~~bash
cargo test -p soma-domain agent_runtime
cargo test -p soma-application agent_runtime
cargo test -p soma bootstrap
cargo test -p soma --test architecture_boundaries
cargo tree -p soma-domain
~~~

## AR-03: Durable run control

### Store and state machine

- migration checksum and rollback fixture;
- create/get/list pagination;
- optimistic state-version conflict;
- every allowed and denied transition;
- state transition and outbox insertion commit atomically;
- terminal state immutable;
- cleanup failure distinct from run failure;
- idempotency key returns prior successful creation;
- parent/child retry and attempt identity;
- artifact/output receipt publication;
- cancellation before and during each phase.

### Recovery

Inject process failure after each durable step:

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

Verify lease expiry, watchdog recovery, no duplicate external resource, recorded step replay, and compensating cleanup.

### Commands

~~~bash
cargo test -p <shared-jobs-crate>
cargo test -p soma-runtime agent_runtime
cargo test -p soma-application agent_runtime_run
~~~

## AR-04: Snippets

### Donor parity

Port and preserve LABBY tests for:

- Markdown frontmatter;
- JavaScript extraction;
- list/get/resolve/create/promote/remove;
- input validation;
- source precedence;
- path and symlink safety;
- promotion metadata.

### Soma-specific

- 20 KiB executable code cap;
- 32 resolves/run and 256 KiB resolved-byte cap;
- recursion and depth limits;
- dependency cycles;
- semver requirement selection;
- missing skill/context/tool/snippet requirement;
- risk class exceeds policy;
- output schema failure;
- artifact quotas and receipts;
- collision and byte-identical duplicate behavior;
- inline, stack, APM, user, built-in precedence.

### Commands

~~~bash
cargo test -p soma-codemode snippet
cargo test -p soma-integrations snippets
cargo test -p soma-provider-adapters codemode
~~~

## AR-05 and AR-06: Context compilation and materialization

### Compilation

- schema and semantic validation;
- import and view cycles;
- repository revision and dirty-state policies;
- required versus optional source availability;
- source freshness and stale classification;
- authorization applied before each retrieval lane and counts;
- deterministic plan digest and ordering;
- exact/FTS/vector/graph/memory fusion fixtures;
- canonical evidence hydration;
- graph depth/entity budgets;
- conflict preservation;
- truncation report;
- immutable parent and child enrichment generation;
- same pinned snapshots produce equivalent compile result.

### Materialization

- manifest and briefing stable output;
- path-to-canonical-reference index;
- read-only context tree;
- symlink escape rejection;
- large raw data stays handle-only by default;
- raw materialization requires disclosure receipt;
- receipt digest, size, content type, item count, and context generation;
- reference, portable, and forensic mode requirements;
- deletion of projection leaves canonical evidence intact.

### Commands

~~~bash
cargo test -p <context-v1-query-crates>
cargo test -p soma-application context
cargo test -p soma-runtime materialization
cargo test -p soma-integrations context
~~~

## AR-07: LABBY loadouts

### Contract

- loadout schema and semantic validation;
- allow/deny intersection;
- wildcard denial;
- required and optional upstream/tool handling;
- virtual-server surface restrictions;
- unhealthy/quarantined status;
- scopes and mutation class;
- catalog pin and explicit refresh generation;
- token/run/agent/subject/expiry binding;
- token revocation;
- physical mode stable unsupported error.

### Live verification

Through LABBY:

1. resolve a read-only loadout;
2. list tools and verify only allowed entries appear;
3. call one safe read-only tool;
4. attempt a denied tool and verify server-side denial;
5. inspect usage/journal evidence;
6. expire or revoke the run policy and verify calls fail.

Report gateway target, catalog generation, selected tool count, denied tool, and safe invocation result.

## AR-08: Incus

### Unit/API

- exec request validation;
- command/env/cwd serialization;
- stdout/stderr limits and truncation;
- timeout and cancellation;
- operation failure and exit-code extraction;
- file path validation and size caps;
- push/pull modes and ownership;
- state/resource decoding;
- operation wait completion/error/timeout;
- local Unix-socket-only guard.

### Live integration

Use an isolated test project/profile:

~~~text
create -> wait -> start -> push -> exec hostname/whoami/platform
-> inspect state/resources -> pull -> stop -> delete
~~~

Confirm exact target identity before further actions. Record instance, project, image, operation IDs, command, exit code, and cleanup result.

## AR-09: Codex adapter

### Unit/mock protocol

- <code>SessionOptions</code> propagation;
- Unix connection and spawn modes;
- initialize/start-thread/run-turn sequence;
- event capacity and call timeout;
- thread/turn IDs captured;
- approval allow, deny, timeout, and cancellation;
- diff/error/terminal status mapping;
- output byte cap and schema validation;
- supervisor bootstrap validation;
- no secrets in transcript/events;
- protocol types remain in integration boundary.

### Container integration

Start a test supervisor and Codex app-server in Incus with a synthetic read-only workspace. Run a deterministic prompt that produces a schema fixture. Verify lifecycle events and process cleanup.

## AR-10: Disclosure

- bootstrap contains only declared capsule;
- mounted path not counted as disclosed;
- summary before raw;
- source/domain/entity/finding selectors;
- raw and expanded levels;
- sensitivity and authorization denial;
- denial does not reveal protected existence or IDs;
- budget narrowing and authorized omitted items;
- approval-required flow and expiry;
- receipt representation digest;
- replay with original receipts;
- tool/skill catalog disclosure levels;
- lifecycle events for every decision.

## AR-11: Cortex lifecycle

- event schema and unknown-kind storage;
- event outbox retries and idempotency;
- run sequence numbers;
- event/ingestion time and clock skew;
- raw versus derived event classes;
- command, tool, snippet, disclosure, claim, research, artifact, Incus correlations;
- secret/redaction fixtures;
- heartbeat raw and aggregate retention;
- graph projection retains canonical event evidence;
- incident timeline query reconstructs what was known before an action.

### Live verification

Run one synthetic agent lifecycle and query it through Cortex timeline and graph surfaces. Record event count, missing events, graph nodes/edges, and correlation IDs.

## AR-12: Context-aware Code Mode

- input reaches snippet as immutable <code>input</code>;
- application and provider Code Mode share the protocol;
- scoped host lists only effective tools;
- caller/run/context/scope identity on calls;
- context catalog, entity, neighborhood, timeline, evidence, comparison, materialization, disclosure, research, artifact actions;
- step record/replay after simulated resume;
- semantic lookup authorization;
- artifact receipt and UI link;
- Code Mode budgets and error taxonomy preserved;
- example snippet executes and validates output.

## AR-13: Axon research and synthesis

- research question normalization and dedup digest;
- evidence-required creation;
- max depth one and parallel job bound;
- durable job lifecycle and cancellation;
- Axon retrieval planning and context bundle reuse;
- primary-source policy and source diversity;
- canonical citation hydration;
- graph candidates with <code>derivedFrom</code> evidence;
- child context generation;
- original context immutable;
- unsupported claims become unknown/rejected;
- contradicting evidence preserved;
- budget and insufficient-evidence statuses;
- structured result validates before narrative generation;
- deterministic calculations and planning against pinned inputs.

## AR-14: APM

- executable identity and version probe;
- exact supported CLI flags verified against pinned/installed APM;
- manifest and lock parsing;
- lock required/missing;
- integrity and audit pass/fail;
- drift detection;
- primitive inventory and selected primitive hashes;
- cache key and byte-identical cache reuse;
- timeout/cancellation/output caps;
- environment clearing and allowlist;
- package hooks remain inert;
- MCP dependency does not become a LABBY capability automatically;
- package cannot broaden stack policy.

## AR-15: Surface and E2E

### Surface parity

For every public operation, compare CLI, REST, MCP, and web/client outputs for IDs, state, error code, authorization, and progress semantics.

### End-to-end fixture

Run <code>docs/agent-runtime/examples/soma.stack.yaml</code> and verify:

1. all sources and schemas;
2. APM receipt;
3. compiled context;
4. LABBY loadout;
5. Incus target identity and health;
6. Codex session;
7. bootstrap disclosure;
8. example snippet and timeline artifact;
9. dependent Axon research;
10. child context;
11. structured synthesis;
12. Markdown briefing;
13. evidence and citation verification;
14. Cortex lifecycle timeline/graph;
15. run manifest;
16. cleanup or retained snapshot.

### Failure E2E scenarios

- missing required source;
- LABBY unavailable;
- denied required tool;
- Incus socket unavailable;
- image/profile missing;
- Codex bootstrap failure;
- approval request in read-only run;
- research budget exhausted;
- Cortex temporarily unavailable;
- output schema invalid;
- cleanup failure.

Each scenario must end in the correct durable state with inspectable evidence and no falsely reported success.
