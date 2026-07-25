# `soma-jobs`

**Proposed path:** `crates/shared/runtime/jobs`  
**Delivery phase:** Runtime  
**Publication:** Publishable after product runners are fully removed.

## Purpose

Generic durable asynchronous job runtime, independent of knowledge and observation domains.

## Donor material

- Axon: axon-jobs
- Cortex: background writer/supervisor health patterns

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Job records and attempts
- Stages, events and progress
- Priorities and deadlines
- Cancellation
- Runner registry
- Worker claiming
- Heartbeats
- Retries/cooldown
- Stale-run recovery
- Scheduling/watch primitives
- In-memory and optional SQLite stores

## Explicit exclusions

- Source adapters
- RAG
- Ledger
- Memory
- Soma job kinds
- Global provider manager

## Public API candidates

- `JobStore`
- `JobRunner`
- `RunnerRegistry`
- `JobRuntime`
- `JobContext`
- `JobRecord`
- `JobEvent`
- `RetryPolicy`
- `CancellationHandle`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- tokio
- sqlx optional
- tracing optional

## Feature plan

- `runtime`
- `sqlite`
- `scheduler`
- `serde`
- `schema`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- claiming contention
- crash recovery
- cancellation
- deadline enforcement
- retry classification
- SQLite migration and restart tests

## Initial Soma consumers

- Soma knowledge refresh
- semantic projection workers
- observation maintenance
- graph rebuilds
- memory jobs

## Extraction acceptance

```text
[ ] Donor paths and exact source baseline recorded
[ ] Neutral API accepted
[ ] Donor fixtures copied or recreated
[ ] Pure implementation moved
[ ] Product/config dependencies removed
[ ] Optional backend adapters implemented
[ ] Soma integration proves real use
[ ] External consumer fixture passes
[ ] Package contents reviewed
[ ] Publication gate passes
```

## Deferred work

Features not required by a v1 vertical slice remain deferred rather than represented by placeholder public APIs. The crate MUST NOT add APM, worker-agent, Incus mission, or Orchestrator concepts.
