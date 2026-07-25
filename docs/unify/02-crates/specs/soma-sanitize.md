# `soma-sanitize`

**Proposed path:** `crates/shared/context/sanitize`  
**Delivery phase:** Foundation  
**Publication:** Publishable leaf crate.

## Purpose

Configurable, bounded, secret-safe transformation primitives for untrusted data.

## Donor material

- Cortex: ingest_metadata.rs and safe excerpts
- Cortex: URL and subprocess diagnostic redaction
- Axon: redaction and payload guards

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Recursive JSON redaction
- Bounded JSON and metadata
- Safe excerpts
- URL credential redaction
- Control-character handling
- Secret-safe Debug wrappers
- Truncation and redaction reports

## Explicit exclusions

- Soma-specific secret-key lists
- Product logging policy
- Storage retention
- Authentication

## Public API candidates

- `RedactionPolicy`
- `JsonLimits`
- `SanitizeOutcome`
- `SafeExcerpt`
- `SafeUrl`
- `SecretDebug`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- serde_json (optional)
- regex or aho-corasick as implementation detail
- url (optional)

## Feature plan

- `json`
- `url`
- `regex`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- property tests for byte bounds
- secret corpus regression tests
- nested JSON fuzz tests
- URL redaction fixtures
- Debug leakage tests

## Initial Soma consumers

- all ingestion and provider crates
- soma-process
- soma-rag
- soma-graph
- Soma API error projection

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
