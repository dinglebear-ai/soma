# `soma-transcript`

**Proposed path:** `crates/shared/context/transcript`  
**Delivery phase:** Cross-domain  
**Publication:** Publishable high-priority convergence crate.

## Purpose

One typed, provider-neutral model and parser family for AI sessions.

## Donor material

- Axon: AI-session adapters and decoders
- Cortex: Claude/Codex/Gemini scanners and tool/skill/hook evidence extraction

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Session and turn model
- Claude, Codex and Gemini dialect parsers
- Tool calls/results
- MCP, skill and hook invocations
- Workspace/project identity
- Malformed-record diagnostics
- Rendering to text/Markdown
- Session discovery/checkpoints optional
- Projection helpers to documents and observations

## Explicit exclusions

- Vector storage
- Agent orchestration
- Global session retention
- Product memory promotion

## Public API candidates

- `Session`
- `Turn`
- `ContentBlock`
- `ToolCall`
- `ToolResult`
- `SkillInvocation`
- `HookInvocation`
- `SessionParser`
- `SessionDialect`
- `SessionProvenance`
- `SessionProjector`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- soma-sanitize
- serde_json

## Feature plan

- `claude`
- `codex`
- `gemini`
- `discovery`
- `render`
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

- provider golden fixtures
- malformed JSONL
- single-document Gemini
- redaction
- stable session/turn IDs
- document and observation projection parity

## Initial Soma consumers

- soma-sources sessions
- soma-collectors sessions
- soma-rag
- soma-graph

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
