# Shared Crate Dependency Graph

## Normative layering

Arrows point from a consumer to the crate it depends on.

```text
soma-process       ──► soma-sanitize
soma-route         ──► soma-primitives
soma-crawl         ──► soma-process
soma-crawl         ──► soma-sanitize

soma-sources       ──► soma-primitives
soma-sources       ──► soma-route
soma-sources       ──► soma-sanitize
soma-sources       ──► soma-process
soma-sources       ──► soma-transcript
soma-sources       ──► soma-crawl          optional web feature

soma-ledger        ──► soma-primitives
soma-jobs          ──► soma-primitives
soma-llm           ──► soma-sanitize       optional

soma-rag           ──► soma-primitives
soma-rag           ──► soma-sanitize
soma-rag           ──► soma-llm            optional synthesis feature

soma-transcript    ──► soma-primitives
soma-transcript    ──► soma-sanitize

soma-observations  ──► soma-primitives
soma-observations  ──► soma-sanitize
soma-ingest        ──► soma-observations
soma-ingest        ──► soma-primitives

soma-collectors    ──► soma-observations
soma-collectors    ──► soma-ingest
soma-collectors    ──► soma-transcript
soma-collectors    ──► soma-sanitize
soma-collectors    ──► soma-process         optional process-backed collectors

soma-graph         ──► soma-primitives
soma-graph         ──► soma-sanitize

soma-memory        ──► soma-primitives
soma-memory        ──► soma-rag             narrow retrieval port
soma-memory        ──► soma-llm             optional extraction feature
```

`Soma` product runners may compose `soma-jobs` with `soma-ledger`, `soma-sources`, `soma-rag`, `soma-graph`, or `soma-memory`. The reusable `soma-jobs` crate itself MUST NOT depend on those domain crates.

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

Cross-cutting
├── soma-jobs
└── soma-graph
```

The family grouping is organizational. The explicit dependency list above is normative.

## Prohibited dependency directions

- `soma-primitives` MUST NOT depend on any other proposed domain crate.
- `soma-jobs` MUST NOT depend on `soma-sources`, `soma-rag`, `soma-ledger`, `soma-graph`, or `soma-memory`.
- `soma-rag` MUST NOT depend on `soma-sources` or `soma-collectors`.
- Observation crates MUST NOT depend on source-generation semantics.
- Source crates MUST NOT depend on observation-stream semantics.
- No shared crate may depend on Soma product crates or surface crates.
- Integration clients MUST remain independently usable and MUST NOT depend on product crates.

## Allowed product composition

```text
crates/soma/application
    ├── knowledge use cases
    ├── observation use cases
    ├── graph/context use cases
    └── memory use cases

crates/soma/runtime
    ├── SQLite stores
    ├── Qdrant/TEI clients
    ├── job runners
    ├── receivers/collectors
    └── context broker construction

apps/soma
    └── executable lifecycle and concrete wiring
```

## CI enforcement

`cargo xtask context-contracts dependency-graph --check` MUST fail when:

- a lower layer imports a higher layer;
- a shared crate imports `crates/soma/*` or `apps/*`;
- a public crate contains an unpublished path dependency;
- product configuration types leak into a shared public API;
- a heavy backend becomes a mandatory default dependency without an ADR.
