# `soma-observations`

**Proposed path:** `crates/shared/observations/model`  
**Delivery phase:** Observation foundation  
**Publication:** Publishable.

## Purpose

Neutral canonical observation model and store interfaces for time-oriented operational data.

## Donor material

- Cortex: db/log models, heartbeat, inventory, graph source records
- Cortex: retention and source identity contracts

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- ObservationRecord
- ObservationKind
- Severity/facility
- Source and subject context
- Structured attributes
- Time and receive metadata
- Canonical observation IDs
- ObservationStore query/write contracts
- Retention classes
- Projection status/outbox records

## Explicit exclusions

- Syslog parsing
- SQLite schema implementation
- Vectorization policy
- Soma host/service entity policy

## Public API candidates

- `ObservationRecord`
- `ObservationBatch`
- `ObservationStore`
- `ObservationQuery`
- `ObservationKind`
- `Severity`
- `SourceContext`
- `RetentionClass`
- `SemanticProjectionTask`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- soma-sanitize
- serde optional

## Feature plan

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

- serialization
- stable identity
- time ordering
- attribute bounds
- query contract fake store

## Initial Soma consumers

- soma-ingest
- soma-collectors
- soma-graph
- Soma observations application

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
