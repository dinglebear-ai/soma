# `soma-ledger`

**Proposed path:** `crates/shared/knowledge/ledger`  
**Delivery phase:** Knowledge foundation  
**Publication:** Publishable.

## Purpose

Authoritative source-generation state machine with manifest diffing, leases, publication fences, and cleanup debt.

## Donor material

- Axon: axon-ledger

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Source registration
- Sequential generations
- Committed baseline
- Manifest validation/diff
- Document state
- Leases and heartbeats
- Generation completion/publication state
- Cleanup debt
- In-memory store
- Optional SQLite store and migrations

## Explicit exclusions

- Vector deletion
- Graph cleanup
- Crawling
- Chunking
- Jobs
- Product notification policy

## Public API candidates

- `SourceLedger`
- `LedgerStore`
- `GenerationState`
- `GenerationLease`
- `DocumentState`
- `CleanupDebt`
- `ManifestDiff`
- `PublicationReceipt`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- sqlx optional
- async-trait or trait futures

## Feature plan

- `memory`
- `sqlite`
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

- state-machine transition tests
- concurrent generation race tests
- lease expiration
- idempotent cleanup debt
- SQLite migration tests
- crash recovery

## Initial Soma consumers

- Soma knowledge application
- soma-jobs product runners

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
