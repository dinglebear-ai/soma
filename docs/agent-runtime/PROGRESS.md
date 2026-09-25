---
title: "Soma Agent Runtime Progress"
created: 2026-08-05
updated: 2026-08-05
doc_type: "progress"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Progress

## Status legend

- **not-started**: no implementation work accepted;
- **in-progress**: one active implementation slice;
- **blocked**: dependency or decision prevents progress;
- **implemented**: code and focused tests landed;
- **verified**: end-to-end acceptance evidence recorded;
- **deferred**: intentionally outside the current delivery horizon.

## Package status

| Area | Status | Evidence |
|---|---|---|
| High-level overview | verified | <code>OVERVIEW.md</code>, <code>ARCHITECTURE.md</code> |
| Baselines and donor map | verified | <code>BASELINES.md</code>, <code>CODE-MAP.md</code> |
| Product specifications | verified | 12 documents under <code>specs/</code> |
| Contracts | verified | 9 documents under <code>contracts/</code> |
| Type blueprints | verified | 6 documents under <code>types/</code> |
| Aggregate models | verified | 6 documents under <code>models/</code> |
| JSON Schemas | verified | 9 schemas under <code>schemas/</code> |
| Examples | verified | Stack, context, loadout, snippet, context output, run output, synthesis output |
| Example/schema validation | verified | All seven example instances validated on 2026-08-05 |
| Implementation plan | in-progress | <code>IMPLEMENTATION-PLAN.md</code> and <code>implementation/</code> |
| Product implementation | not-started | No runtime code added by this documentation package |

## Delivery milestones

| ID | Milestone | Status | Exit evidence |
|---|---|---|---|
| AR-00 | Contracts and schemas | verified | Schemas and fixtures validate |
| AR-01 | Appdata and config normalization | not-started | All runtime paths derive from <code>default_data_dir()</code> |
| AR-02 | Domain and application boundaries | not-started | DTOs, ports, stores, use cases compile with unavailable adapters |
| AR-03 | Durable run control | not-started | Recoverable run state, leases, events, cancellation, artifacts |
| AR-04 | Soma snippet store | not-started | LABBY-derived store and promotion on shared Code Mode |
| AR-05 | Context manifest compiler | not-started | Manifest validation and immutable context generation |
| AR-06 | Context materialization | not-started | Manifest, briefing, filesystem, graph, JSONL, Code Mode handles |
| AR-07 | LABBY loadout adapter | not-started | Server-side scoped catalog and call enforcement |
| AR-08 | Incus workload operations | not-started | Exec, transfer, state, wait helpers with tests |
| AR-09 | Codex assistant adapter | not-started | One-shot Codex run inside Incus |
| AR-10 | Progressive disclosure | not-started | Recorded disclosure decisions and receipts |
| AR-11 | Lifecycle observation | not-started | Cortex-compatible event ingestion and correlation |
| AR-12 | Context-aware Code Mode | not-started | Run-scoped context tools and snippets |
| AR-13 | Dependent Axon research synthesis | not-started | Child context generation from durable research job |
| AR-14 | APM package adapter | not-started | Locked package resolution and audit receipt |
| AR-15 | Read-only vertical slice | not-started | Example stack runs end to end |
| AR-16 | Resident assistants | deferred | Durable multi-run instance and workspace |
| AR-17 | Multi-service stacks | deferred | Dependency-aware service orchestration |
| AR-18 | Remote Incus | deferred | Secure mTLS implementation and threat-model verification |

## Work-in-progress rule

Only one vertical runtime milestone and one narrowly required shared-foundation task may be active simultaneously. Do not scaffold every proposed module before AR-15 starts producing an end-to-end result.

## Updating this tracker

Every status change must include:

- pull request or commit;
- baseline commits used;
- files changed;
- tests run and results;
- runtime verification;
- known deviations from the specification;
- rollback or migration notes;
- links to generated schema, OpenAPI, or MCP contract changes.

A milestone reaches **verified** only after its real product surface and runtime behavior are exercised, not when unit tests alone pass.
