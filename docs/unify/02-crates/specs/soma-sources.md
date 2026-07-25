# `soma-sources`

**Proposed path:** `crates/shared/knowledge/sources`  
**Delivery phase:** Knowledge ingestion  
**Publication:** Publishable, with minimal default features.

## Purpose

One feature-gated adapter SDK and implementations for Axon's refreshable source families.

## Donor material

- Axon: axon-adapters
- Axon: selected vertical normalization from axon-extract

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- SourceAdapter contract
- Adapter descriptor and capabilities
- Discover/acquire/normalize lifecycle
- Local files and directories
- Git and GitHub sources
- Feeds
- YouTube
- Reddit
- Package registries
- Uploads
- AI-session source adapter
- Tool output sources
- Deterministic fake adapters

## Explicit exclusions

- Web crawling engine
- Chunking/embedding
- Generation persistence
- Soma jobs or API routes
- Provider credentials storage

## Public API candidates

- `SourceAdapter`
- `AdapterRegistry`
- `AdapterDescriptor`
- `DiscoveryContext`
- `AcquisitionContext`
- `SourceAcquisition`
- `AdapterError`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- soma-route
- soma-sanitize
- soma-process
- soma-transcript
- soma-crawl optional
- service clients behind features

## Feature plan

- `local`
- `git`
- `github`
- `feed`
- `youtube`
- `reddit`
- `crates-io`
- `npm`
- `pypi`
- `sessions`
- `upload`
- `tools`
- `web`
- `all`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- per-adapter manifest fixtures
- changed-only acquisition
- normalization golden tests
- credential redaction
- offline fake adapter
- feature matrix

## Initial Soma consumers

- Soma knowledge application and job runners

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
