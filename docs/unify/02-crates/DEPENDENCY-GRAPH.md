---
title: "Shared Crate Dependency Graph"
created: 2026-07-24
updated: 2026-07-31
---

# Shared Crate Dependency Graph

## Normative layering

Arrows point from a consumer to the crate it depends on.

```text
soma-process       -> soma-sanitize
soma-route         -> soma-primitives
soma-crawl         -> soma-process
soma-crawl         -> soma-sanitize

soma-sources       -> soma-primitives
soma-sources       -> soma-route
soma-sources       -> soma-sanitize
soma-sources       -> soma-process
soma-sources       -> soma-transcript
soma-sources       -> soma-crawl          optional web feature

soma-ledger        -> soma-primitives
soma-jobs          -> soma-primitives
soma-llm           -> soma-sanitize       optional

soma-rag           -> soma-primitives
soma-rag           -> soma-sanitize
soma-rag           -> soma-llm            optional synthesis feature

soma-transcript    -> soma-primitives
soma-transcript    -> soma-sanitize

soma-observations  -> soma-primitives
soma-observations  -> soma-sanitize
soma-ingest        -> soma-observations
soma-ingest        -> soma-primitives

soma-collectors    -> soma-observations
soma-collectors    -> soma-ingest
soma-collectors    -> soma-transcript
soma-collectors    -> soma-sanitize
soma-collectors    -> soma-process         optional process-backed collectors

soma-graph         -> soma-primitives
soma-graph         -> soma-sanitize

soma-memory        -> soma-primitives
soma-memory        -> soma-rag             narrow retrieval port
soma-memory        -> soma-llm             optional extraction feature

soma-ops           -> soma-primitives
soma-ops           -> soma-sanitize
soma-fleet         -> soma-ops
soma-fleet         -> soma-sanitize
soma-fleet         -> soma-process         optional process-backed transport
soma-infra         -> soma-ops
soma-infra         -> soma-fleet
soma-infra         -> soma-sanitize
soma-infra         -> soma-process         optional command driver
soma-infra         -> incus-client         optional Incus feature
```

Product runners may compose shared engines with `soma-jobs`. The reusable jobs crate itself MUST NOT depend on product runners or domain adapters.

## Dependency families

```text
Foundation
├── soma-primitives
├── soma-sanitize
└── soma-process

Knowledge
├── soma-route
├── soma-sources
├── soma-crawl
└── soma-ledger

Semantic
├── soma-llm
├── soma-rag
├── soma-transcript
└── soma-memory

Observations
├── soma-observations
├── soma-ingest
└── soma-collectors

Operations
├── soma-ops
├── soma-fleet
├── soma-infra
└── incus-client

Cross-cutting
├── soma-jobs
└── soma-graph
```

The family grouping is organizational. The explicit dependency list above is normative.

## Prohibited dependency directions

- `soma-primitives` MUST NOT depend on any other proposed domain crate.
- `soma-jobs` MUST NOT depend on source, RAG, ledger, graph, memory, collector, fleet, or infrastructure engines.
- `soma-rag` MUST NOT depend on `soma-sources` or `soma-collectors`.
- Observation crates MUST NOT depend on source-generation or operation-execution semantics.
- Source crates MUST NOT depend on observation-stream or operation-execution semantics.
- `soma-ops` MUST NOT contain Synapse scopes, Soma principals, MCP elicitation, or product prompts.
- `soma-fleet` MUST NOT choose product host-discovery defaults.
- `soma-infra` MUST NOT depend on RMCP, Axum, product auth, web frameworks, or Cortex stores.
- No shared crate may depend on any product or surface crate.
- Integration clients MUST remain independently usable and MUST NOT depend on product crates.

## Allowed product composition

```text
crates/<product>/application
    -> defines use cases and ports

crates/<product>/integrations or runtime
    -> implements ports over shared engines or remote clients

crates/<product>/{cli,api,mcp,web}
    -> calls product application use cases only

apps/<product>
    -> executable lifecycle and concrete wiring
```

Soma may choose embedded or remote implementations for Axon, Cortex, and Synapse capabilities. A standalone product never depends on Soma product crates.

## CI enforcement

Architecture checks MUST fail when:

- a lower layer imports a higher layer;
- a shared crate imports any `crates/<product>/*` or `apps/*` path;
- a standalone product imports `crates/soma/*` or `apps/soma`;
- a public crate contains an unpublished path dependency;
- product configuration types leak into a shared public API;
- product environment variables are read inside shared crates;
- a product surface reaches database, Docker, SSH, or Incus clients directly;
- a heavy backend becomes a mandatory default dependency without an ADR.
