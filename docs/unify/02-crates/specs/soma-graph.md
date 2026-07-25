# `soma-graph`

**Proposed path:** `crates/shared/context/graph`  
**Delivery phase:** Context intelligence  
**Publication:** Publishable flagship crate after shared vocabulary seam is proven.

## Purpose

Evidence-first temporal graph kernel joining knowledge, infrastructure, sessions, tools, and observations.

## Donor material

- Axon: axon-graph
- Cortex: investigation graph contracts, entity resolver, trust/confidence and explanations

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Entities and aliases
- Relationships and claims
- Evidence references
- Authority/trust/confidence
- Temporal validity
- Deterministic IDs
- Observation resolution and merging
- Conflict preservation
- Bounded neighborhood/path queries
- Projection lifecycle
- Community and summary interfaces
- In-memory and optional SQLite store

## Explicit exclusions

- Axon-only or Cortex-only vocabulary
- Raw record storage
- LLM extraction policy
- Soma context query planner

## Public API candidates

- `EvidenceGraph`
- `GraphStore`
- `GraphVocabulary`
- `GraphCandidate`
- `Entity`
- `EntityAlias`
- `Relationship`
- `Claim`
- `EvidenceRef`
- `TrustLevel`
- `AuthorityClass`
- `GraphQuery`
- `EvidencePath`
- `ProjectionVersion`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- soma-sanitize
- sqlx optional
- petgraph optional for offline algorithms

## Feature plan

- `sqlite`
- `algorithms`
- `communities`
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

- stable IDs
- merge/conflict fixtures
- alias resolution
- temporal validity
- bounded traversal
- evidence requirement
- confidence math
- SQLite migration
- community algorithm fixtures

## Initial Soma consumers

- Soma context broker
- knowledge and observation projectors
- soma-memory

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
