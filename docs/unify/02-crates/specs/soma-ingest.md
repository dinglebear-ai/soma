# `soma-ingest`

**Proposed path:** `crates/shared/observations/ingest`  
**Delivery phase:** Observation runtime  
**Publication:** Publishable.

## Purpose

Generic reliable stream/batch ingestion engine with acknowledgement, checkpointing, backpressure, and semantic outbox.

## Donor material

- Cortex: batched SQLite writer
- Cortex: file-tail/checkpoint supervision
- Cortex: receiver backpressure and shutdown
- Cortex: retry/health transition patterns

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Bounded queues
- Batching and flush policy
- BatchSink contract
- Acknowledgements
- CheckpointStore contract
- Graceful drain
- Retry classification
- Poison-record isolation
- Health snapshots
- Semantic outbox dispatch
- Optional SQLite observation store/writer

## Explicit exclusions

- Specific wire formats
- Graph extraction
- RAG implementation
- Product alerting

## Public API candidates

- `IngestRuntime`
- `BatchSink`
- `CheckpointStore`
- `IngestPolicy`
- `WriteAck`
- `ErrorClassifier`
- `HealthSnapshot`
- `ProjectionOutbox`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-observations
- soma-primitives
- tokio
- sqlx optional

## Feature plan

- `runtime`
- `sqlite`
- `outbox`
- `serde`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- backpressure
- shutdown drain
- retry/bisect
- poison rows
- checkpoint crash recovery
- outbox idempotency
- SQLite throughput

## Initial Soma consumers

- soma-collectors
- Soma observation runtime

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
