# `soma-llm`

**Proposed path:** `crates/shared/semantic/llm`  
**Delivery phase:** Semantic  
**Publication:** Publishable.

## Purpose

Provider-neutral completion and streaming contracts for RAG synthesis, extraction, summarization, and future product use.

## Donor material

- Axon: axon-llm
- Cortex: assessment-provider execution patterns

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Completion requests/responses
- Streaming events
- Structured output
- Usage and finish reasons
- Model capabilities
- Timeout/cancellation contracts
- Provider health metadata adapters
- Deterministic fake model

## Explicit exclusions

- Prompt policy
- Axon config
- Soma orchestrator
- Codex client implementation already in Soma
- Global singleton reservation state

## Public API candidates

- `LanguageModel`
- `StreamingLanguageModel`
- `CompletionRequest`
- `CompletionResponse`
- `CompletionStreamEvent`
- `ModelCapabilities`
- `LlmError`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- serde optional
- futures-core
- soma-sanitize optional
- existing codex-app-server-client via adapter outside core

## Feature plan

- `streaming`
- `serde`
- `schema`
- `openai-compatible-adapter`
- `gemini-cli-adapter`
- `codex-adapter`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- stream ordering
- structured output fixtures
- cancellation
- usage accounting
- fake provider
- secret-safe errors

## Initial Soma consumers

- soma-rag
- soma-memory
- Soma product synthesis and extraction

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
