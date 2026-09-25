---
title: "Agent Runtime Code Map"
created: 2026-08-05
updated: 2026-08-05
doc_type: "code-map"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Code Map

This map identifies the current code that must be reused, extended, or ported.

## Soma foundations

| Capability | Current code | Required use |
|---|---|---|
| Code Mode configuration and budgets | <code>crates/shared/codemode/src/config.rs</code> | Preserve timeout, response, log, call, and snippet budgets |
| Code Mode host boundary | <code>crates/shared/codemode/src/host.rs</code> | Add run-aware catalog, context, and disclosure behavior through the host |
| Snippet DTOs | <code>crates/shared/codemode/src/snippet/store.rs</code> | Extend with version, skills, context, tools, output, and risk metadata |
| Artifacts | <code>crates/shared/codemode/src/artifacts/</code> | Reuse run-scoped artifact receipts and quotas |
| State workspace | <code>crates/shared/codemode/src/state/</code> | Reuse for bounded agent state; normalize current nested metadata layout later |
| Code Mode product port | <code>crates/soma/integrations/src/codemode.rs</code> | Replace default disabled-only wiring with resolved run configuration |
| Product runtime wiring | <code>apps/soma/src/bootstrap.rs</code> | Construct new ports and stores only here |
| Drop-in providers | <code>crates/soma/application/src/providers/</code> | Preserve tools, prompts, resources, refresh, and security behavior |
| Appdata resolution | <code>crates/soma/config/src/config.rs</code> | Make all new runtime paths derive from <code>default_data_dir()</code> |
| Gateway store | <code>crates/shared/mcp/gateway/src/gateway/config_store.rs</code> | Preserve atomic config and secret writes |
| Gateway paths | <code>crates/shared/mcp/gateway/src/config/defaults.rs</code> | Reuse gateway home validation and nested Soma gateway home |
| Incus client | <code>crates/shared/incus-client/</code> | Reuse instance, snapshot, operation, and event APIs; add only missing workload operations |
| Codex runtime | <code>crates/shared/codex-app-server-client/</code> | Use <code>CodexSession</code>, event stream, approval handling, and turn collection |
| Observability | <code>crates/shared/observability/</code>, <code>crates/shared/traces/</code> | Emit run and disclosure telemetry through existing tracing patterns |
| Operations | <code>crates/shared/operations/infra/</code>, <code>crates/shared/operations/fleet/</code> | Reuse bounded host/container execution and fanout where agent runtime operations need them |
| Application boundary | <code>crates/soma/application/</code> | Own stack resolution, context compilation, run orchestration, and ports |
| Runtime boundary | <code>crates/soma/runtime/</code> | Own concrete stores, background workers, Incus, LABBY, and agent adapters |
| Surfaces | <code>crates/soma/cli</code>, <code>crates/soma/api</code>, <code>crates/soma/mcp</code>, <code>crates/soma/web</code> | Thin projections of the same application use cases |

## Axon donor map

| Capability | Pinned donor paths |
|---|---|
| Retrieval request, plan, result | <code>crates/axon-retrieval/src/query.rs</code>, <code>plan.rs</code> |
| Context bundle and ask context | <code>crates/axon-retrieval/src/context.rs</code>, <code>ask_context.rs</code> |
| Query service | <code>crates/axon-retrieval/src/service.rs</code> |
| Synthesis pipeline | <code>crates/axon-services/src/query/synthesis/</code> |
| Durable jobs | <code>crates/axon-jobs/src/unified/</code>, <code>workers/</code>, <code>state_machine.rs</code> |
| Graph evidence and candidates | <code>crates/axon-graph/src/candidate.rs</code>, <code>evidence.rs</code>, <code>edge.rs</code> |
| Source manifests and generations | <code>crates/axon-ledger/src/manifest.rs</code>, <code>generation.rs</code> |
| Memory | <code>crates/axon-memory/src/</code> |
| Codex-backed LLM runtime | <code>crates/axon-llm/src/runtime/codex_app_server/</code> |
| Pipeline observation | <code>crates/axon-observe/src/</code> |

## Cortex donor map

| Capability | Pinned donor paths |
|---|---|
| Canonical log/session models | <code>src/db/models.rs</code> |
| Heartbeats and windows | <code>src/db/heartbeat.rs</code> |
| Evidence graph and multi-hop traversal | <code>src/db/graph.rs</code> |
| Log ingest | <code>src/db/ingest.rs</code>, <code>src/receiver/</code> |
| Journald and syslog forwarding | <code>src/agent/journald.rs</code>, <code>syslog_file.rs</code> |
| Shell history | <code>src/agent/shell_history.rs</code> |
| AI transcripts | <code>src/agent/ai_transcript.rs</code> |
| Docker log lifecycle | <code>src/docker_ingest/</code> |
| OTLP | <code>src/otlp/</code> |
| Inventory and raw configurations | <code>src/inventory/</code> |
| Incident and context queries | <code>src/db/models.rs</code>, <code>src/mcp/tools/context.rs</code> |

## LABBY donor map

| Capability | Pinned donor paths |
|---|---|
| Snippet filesystem store | <code>crates/labby-codemode/src/snippet/store.rs</code> |
| Snippet actions and promotion | <code>crates/labby/src/dispatch/snippets/</code> |
| Gateway Code Mode host | <code>crates/labby-gateway/src/gateway/code_mode/code_mode_host.rs</code> |
| Gateway config mutation | <code>crates/labby-gateway/src/gateway/config.rs</code> |
| Virtual server surface policy | <code>crates/labby-gateway/src/gateway/manager/virtual_servers.rs</code> |
| Runtime views and exposure rows | <code>crates/labby-gateway/src/gateway/types.rs</code> |
| Config and workspace paths | <code>crates/labby/src/config/paths.rs</code> |
| Incus setup UX | <code>crates/labby/src/cli/incus.rs</code>, <code>dispatch/setup/incus.rs</code> |

## APM integration map

At commit <code>dcbaf654...</code>, APM provides:

- <code>apm.yml</code> project manifests;
- <code>apm.lock.yaml</code> resolved dependency locks;
- primitives for instructions, skills, prompts, agents, hooks, plugins, and MCP servers;
- install, compile, audit, pack, policy, integrity, and drift workflows.

Soma must consume those outputs through a process adapter and record their hashes. It must not copy APM's package resolver into Rust in the first implementation.
