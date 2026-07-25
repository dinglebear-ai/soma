# `soma-primitives`

**Proposed path:** `crates/shared/context/primitives`  
**Delivery phase:** Foundation  
**Publication:** Publishable leaf crate after crates.io availability review.

## Purpose

Small transport-neutral vocabulary shared by knowledge, observation, graph, memory, and retrieval crates.

## Donor material

- Axon: axon-api source/document/generation DTOs
- Axon: axon-error diagnostic projection patterns
- Cortex: graph and observation contracts

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Stable typed identifiers
- CanonicalRef and Citation
- SourceRequest, ResolvedSource, SourceManifest and diff records
- SourceDocument and PreparedChunk envelopes
- PipelineStage, ProgressEvent and Diagnostic projections
- Content hashes, source locators and bounded metadata

## Explicit exclusions

- Storage implementations
- HTTP/MCP/CLI DTO envelopes
- Qdrant or SQLite clients
- Product configuration
- Soma authorization policy

## Public API candidates

- `CanonicalRef`
- `Citation`
- `SourceId`
- `GenerationId`
- `DocumentId`
- `ChunkId`
- `ObservationId`
- `JobId`
- `ArtifactId`
- `SourceRequest`
- `ResolvedSource`
- `SourceManifest`
- `SourceManifestDiff`
- `SourceDocument`
- `PreparedChunk`
- `PipelineDiagnostic`
- `ProgressEvent`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- serde (optional)
- schemars (optional)
- time or chrono behind feature
- uuid or stable hash utility

## Feature plan

- `serde`
- `schema`
- `chrono`
- `uuid`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- stable ID golden tests
- schema fixtures
- serialization round trips
- locator validation
- bounded metadata property tests

## Initial Soma consumers

- soma-route
- soma-sources
- soma-ledger
- soma-rag
- soma-observations
- soma-graph
- Soma product context modules

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
