# `soma-process`

**Proposed path:** `crates/shared/runtime/process`  
**Delivery phase:** Foundation  
**Publication:** Publishable.

## Purpose

Runtime-neutral bounded child-process execution with cancellation, streaming, and safe diagnostics.

## Donor material

- Cortex: bounded process execution and deployment commands
- Axon: Git, Gemini, YouTube and browser subprocess patterns

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Command specifications
- Bounded stdout/stderr capture
- Timeout and cancellation
- Kill and reap
- Streaming line/event output
- Stdin bytes or streams
- Typed exit classification
- Elapsed time and truncation metadata

## Explicit exclusions

- SSH host discovery
- Product environment variables
- Deployment policy
- Shell command construction from untrusted strings

## Public API candidates

- `CommandSpec`
- `RunPolicy`
- `CommandOutput`
- `ProcessRunner`
- `StreamingProcess`
- `ProcessError`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- tokio behind async feature
- soma-sanitize
- thiserror

## Feature plan

- `tokio`
- `streaming`
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

- timeout/kill tests
- large-output truncation tests
- cancellation tests
- stdin tests
- process-tree cleanup tests
- secret-safe diagnostics

## Initial Soma consumers

- soma-crawl
- soma-sources
- soma-llm
- future deployment adapters
- Soma runtime

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
