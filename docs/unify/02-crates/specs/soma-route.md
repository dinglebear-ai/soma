# `soma-route`

**Proposed path:** `crates/shared/knowledge/route`  
**Delivery phase:** Knowledge foundation  
**Publication:** Publishable.

## Purpose

Parse, canonicalize, validate, and classify heterogeneous source targets without performing acquisition.

## Donor material

- Axon: axon-route

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Target parsing
- Canonical origin construction
- Stable source identity
- Source kind and scope selection
- Authority and safety validation
- Adapter selection hints
- Path/URL normalization

## Explicit exclusions

- Network fetching
- Manifest discovery
- Jobs
- Storage
- Soma auth scopes

## Public API candidates

- `SourceRouter`
- `RoutePolicy`
- `RoutedSource`
- `SourceAuthority`
- `RouteError`
- `AdapterHint`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- url
- percent-encoding

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

- routing golden fixtures for every source family
- path traversal cases
- URL normalization
- stable ID compatibility
- malformed input fuzzing

## Initial Soma consumers

- soma-sources
- Soma knowledge application

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
