# `soma-rag`

**Proposed path:** `crates/shared/semantic/rag`  
**Delivery phase:** Semantic  
**Publication:** Publishable flagship crate, minimal defaults and backend features.

## Purpose

Coarse, reusable end-to-end RAG engine over normalized documents.

## Donor material

- Axon: axon-document
- Axon: axon-parse
- Axon: axon-embedding
- Axon: axon-vectors
- Axon: axon-retrieval
- Axon: synthesis and live orchestration mined from axon-services

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- Content classification and chunk routing
- Markdown, code, prose, structured, transcript and session chunkers
- Tree-sitter parsing features
- Embedding contracts and batching
- Dense/sparse vector point construction
- VectorStore contract
- Qdrant adapter
- FTS indexing contract
- Hybrid retrieval and fusion
- Reranking
- Citation/context assembly
- Optional synthesis
- Processing fingerprints
- Deterministic fakes

## Explicit exclusions

- Source acquisition
- Source-generation authority
- Observation ingestion
- Soma context query policy
- HTTP/MCP surfaces
- Product prompts

## Public API candidates

- `IndexPipeline`
- `QueryPipeline`
- `DocumentPreparer`
- `Chunker`
- `ChunkRouter`
- `Embedder`
- `VectorStore`
- `Reranker`
- `Synthesizer`
- `IndexRequest`
- `IndexReceipt`
- `QueryRequest`
- `QueryResult`
- `CitationSet`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-primitives
- soma-sanitize
- soma-llm optional
- qdrant-client optional
- tree-sitter grammars optional
- tokenization libs

## Feature plan

- `markdown`
- `code`
- `structured`
- `transcript`
- `tree-sitter-rust`
- `tree-sitter-python`
- `tree-sitter-javascript`
- `tree-sitter-typescript`
- `tei`
- `qdrant`
- `fts`
- `rerank`
- `synthesis`
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

- chunk golden fixtures
- stable boundaries
- language parser fixtures
- embedding batching
- idempotent upsert
- hybrid retrieval evals
- citation hydration
- Qdrant integration
- feature matrix
- fuzz untrusted parsers

## Initial Soma consumers

- Soma knowledge application
- observation semantic projector
- memory recall

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
