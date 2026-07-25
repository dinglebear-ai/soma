# Shared Crate Catalog

**Status:** Proposed v1 extraction boundary  
**Count:** 16 new shared crates  
**Naming:** Every proposed package uses `soma-<one-word>`. The convention is fixed; crates.io availability and fallback names remain to be verified.

The crate boundaries are intentionally coarser than Axon's current 23-crate workspace. They are organized around independently useful capabilities, not donor-internal dependency surgery.

| # | Package | Phase | Responsibility |
|---:|---|---|---|
| 1 | `soma-primitives` | Foundation | Small transport-neutral vocabulary shared by knowledge, observation, graph, memory, and retrieval crates. |
| 2 | `soma-sanitize` | Foundation | Configurable, bounded, secret-safe transformation primitives for untrusted data. |
| 3 | `soma-process` | Foundation | Runtime-neutral bounded child-process execution with cancellation, streaming, and safe diagnostics. |
| 4 | `soma-route` | Knowledge foundation | Parse, canonicalize, validate, and classify heterogeneous source targets without performing acquisition. |
| 5 | `soma-sources` | Knowledge ingestion | One feature-gated adapter SDK and implementations for Axon's refreshable source families. |
| 6 | `soma-crawl` | Knowledge ingestion | Independent bounded web crawling and capture engine usable without RAG. |
| 7 | `soma-ledger` | Knowledge foundation | Authoritative source-generation state machine with manifest diffing, leases, publication fences, and cleanup debt. |
| 8 | `soma-jobs` | Runtime | Generic durable asynchronous job runtime, independent of knowledge and observation domains. |
| 9 | `soma-llm` | Semantic | Provider-neutral completion and streaming contracts for RAG synthesis, extraction, summarization, and future product use. |
| 10 | `soma-rag` | Semantic | Coarse, reusable end-to-end RAG engine over normalized documents. |
| 11 | `soma-transcript` | Cross-domain | One typed, provider-neutral model and parser family for AI sessions. |
| 12 | `soma-memory` | Knowledge intelligence | Evidence-backed durable memory lifecycle and recall engine. |
| 13 | `soma-observations` | Observation foundation | Neutral canonical observation model and store interfaces for time-oriented operational data. |
| 14 | `soma-ingest` | Observation runtime | Generic reliable stream/batch ingestion engine with acknowledgement, checkpointing, backpressure, and semantic outbox. |
| 15 | `soma-collectors` | Observation ingestion | Feature-gated receivers, collectors, and normalizers for Cortex's operational sources. |
| 16 | `soma-graph` | Context intelligence | Evidence-first temporal graph kernel joining knowledge, infrastructure, sessions, tools, and observations. |

## Existing Soma crates that remain authoritative

No replacement or convergence crate is planned for:

- authentication and OAuth;
- gateway and upstream MCP composition;
- provider catalog and Code Mode;
- MCP, REST, CLI, OpenAPI, and web surface projection;
- observability and MCP traces;
- self-update transactional replacement;
- the current application/domain/runtime composition model.

The new crates integrate with those foundations. They do not recreate them.

## Crate families

```text
Foundation
├── soma-primitives
├── soma-sanitize
└── soma-process

Knowledge acquisition
├── soma-route
├── soma-sources
├── soma-crawl
└── soma-ledger

Semantic processing
├── soma-llm
├── soma-rag
├── soma-transcript
└── soma-memory

Operational observations
├── soma-observations
├── soma-ingest
└── soma-collectors

Cross-cutting runtime and intelligence
├── soma-jobs
└── soma-graph
```

## Public boundary rule

A shared crate MUST:

1. be useful without the Soma product binary;
2. avoid dependencies on `crates/soma/*` and `apps/*`;
3. accept explicit configuration rather than reading `SOMA_*`, `AXON_*`, or `CORTEX_*`;
4. expose typed, non-exhaustive errors;
5. place storage and heavy providers behind optional features or traits;
6. provide an independent consumer fixture outside the Soma workspace;
7. pass `cargo package` and `cargo publish --dry-run`;
8. contain no product authorization or surface policy.

## Product composition rule

The following remain product behavior, initially implemented as modules inside Soma's existing `domain`, `application`, and `runtime` crates:

- knowledge source administration;
- observation source administration;
- semantic projection policy;
- context query planning;
- GraphRAG strategy selection;
- cross-store evidence hydration;
- memory promotion policy;
- web/API/MCP/CLI use cases;
- authorization and tenancy;
- service supervision and runtime construction.

## Specs

Each proposed crate has a complete implementation specification under [`specs/`](specs/).
