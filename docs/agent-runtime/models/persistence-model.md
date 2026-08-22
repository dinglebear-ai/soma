---
title: "Agent Runtime Persistence Model"
created: 2026-08-05
updated: 2026-08-05
doc_type: "model"
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

# Persistence Model

## Authority

The agent runtime must not introduce a disconnected “everything database.” Storage follows existing authority boundaries.

| Data | Authority |
|---|---|
| Stack sources, context manifests, snippets, loadouts | Files/packages plus content digests |
| Resolved stack and run control state | Soma control SQLite and durable JSON artifacts |
| Axon documents, source generations, retrieval, research jobs | Axon-derived canonical stores integrated by context v1 |
| Cortex logs, sessions, commands, telemetry, heartbeats, inventory | Cortex-derived canonical observation store |
| Evidence graph | Rebuildable projection over canonical records |
| Code Mode artifacts and agent outputs | Durable artifact store with receipts |
| Compiled context metadata and selections | Soma context store; source bodies remain canonical elsewhere |
| Lifecycle events | Cortex-compatible observation ingestion and run-index projection |
| Narrative synthesis | Derived artifact; structured synthesis result is authoritative output |

## Proposed appdata layout

~~~text
<SOMA_HOME>/
  config.toml
  .env
  providers/
  snippets/
  stacks/
  contexts/
    manifests/
    compiled/
  loadouts/
  packages/
    cache/
  runs/
    <run-id>/
      resolved-stack.json
      compiled-context.json
      disclosure-log.jsonl
      run-manifest.json
      synthesis-result.json
      artifacts/
  code-mode-artifacts/
  state/
  cache/
  logs/
~~~

The exact physical database split follows context-v1 storage architecture. This layout defines stable operator-facing artifacts, not every SQLite file.

## Atomic publication

Resolved manifests, compiled contexts, run manifests, disclosure logs, and synthesis results use temp-file, fsync, rename, parent-sync publication consistent with existing gateway, setup, provider-state, and Tauri persistence code. Secret files use mode 0600 and reject symlinks.

## SQLite

Control tables SHOULD include:

- agent stacks and resolved generations;
- agent runs, attempts, leases, and state transitions;
- external resource bindings;
- compiled-context metadata and selected-item index;
- disclosure requests, decisions, and receipts;
- loadout resolutions;
- snippet definitions and executions;
- synthesis investigations, questions, claims, and result references;
- artifact index and retention state.

Axon's migration-checksum, canonical job, and transaction patterns should be reused.

## Event outbox

State transitions and lifecycle events that must reach Cortex use a transactional outbox. State change and event intent commit together. A worker publishes idempotently and records delivery.

## Retention

Retention is independently configurable for:

- run control metadata;
- compiled contexts;
- portable/forensic packs;
- disclosure logs;
- artifacts;
- raw transcripts and logs;
- heartbeat resolution tiers;
- retained Incus instances and snapshots;
- package caches.

Deleting a projection never deletes canonical evidence without a separate authorized retention operation.

## Backup and restore

Backup includes control databases, manifests, lockfiles, portable retained evidence, artifact metadata, and secrets according to deployment policy. Rebuildable vector and graph projections may be excluded when their canonical inputs and rebuild version are preserved.
