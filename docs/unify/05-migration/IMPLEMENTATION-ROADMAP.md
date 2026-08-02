---
title: "Implementation Roadmap"
created: 2026-07-24
updated: 2026-07-31
---

# Implementation Roadmap

## Phase gates

| Phase | Outcome | Exit gate |
|---:|---|---|
| 0 | Existing gateway + Aurora shell ready | Gateway/admin/web flows are stable |
| 1 | Contracts are executable | Schema, fixture and architecture checks pass |
| 2 | Shared foundations | External consumer fixtures pass |
| 3 | Local knowledge walking skeleton | End-to-end cited search through all surfaces |
| 4 | Durable jobs | Restart/cancel/retry E2E passes |
| 5 | Unified sessions | Same session searchable semantically and operationally |
| 6 | Web crawl | Real docs site indexes through same pipeline |
| 7 | All Axon sources | Adapter capability matrix passes |
| 8 | Observation foundation | Canonical file/syslog search and retention pass |
| 9 | All Cortex sources | Collector/receiver matrix passes |
| 10 | Semantic observations | Outbox recovery and cited vector hydration pass |
| 11 | Evidence graph | Cross-domain paths have canonical evidence |
| 12 | GraphRAG context broker | North-star evidence retrieval succeeds |
| 13 | Memory | Verified memory lifecycle and recall pass |
| 14 | Context cutover | Migration, performance, backup, security and parity gates pass |

## Product-family and operations track

| Phase | Outcome | Exit gate |
|---:|---|---|
| O0 | Multi-distribution contracts | ADRs, donor locks, product boundaries, and architecture checks are accepted |
| O1 | Synapse baseline import | Standalone Synapse builds unchanged from a history-preserving import |
| O2 | Shared operations foundation | `soma-ops` external consumer and 59-operation mapping fixtures pass |
| O3 | Fleet and read-only infrastructure | SSH/fanout security gates and live read-only Docker/host parity pass |
| O4 | Controlled mutations | Plan, authorization evidence, progress, cancellation, verification, and events pass |
| O5 | Incus convergence | Generic Incus operations use the neutral client; Soma deployment policy consumes the operations port |
| O6 | Standalone and integrated cutover | Synapse packaging plus embedded/remote Soma parity and rollback gates pass |

The operations track is additive to the context v1 plan. It does not make autonomous execution part of context v1.

## Recommended PR train per slice

1. Contract/ADR and fixtures.
2. Shared core.
3. Optional infrastructure adapters.
4. Soma application/runtime composition.
5. CLI/API/MCP/Web projection.
6. Donor parity, packaging and operations.

Each PR remains useful and mergeable. No long-lived mega-branch.

## Status source

`capability-matrix.yaml` is canonical. Generated status pages SHOULD be produced by `xtask`.

## Scope change

Any addition of Agent Package Manager, worker-agent dispatch, Incus mission containers, custom images, autonomous PR/deploy, or self-improving skills requires a new versioned program and ADR. It cannot enter v1 as incidental infrastructure.
