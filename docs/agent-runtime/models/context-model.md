---
title: "Context Model"
created: 2026-08-05
updated: 2026-08-05
doc_type: "model"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Context Model

## Three distinct objects

### Context manifest

Desired eligibility, graph scope, policies, saved views, budgets, and materialization preferences. It is version-controlled input.

### Compiled context

Immutable task-scoped selection with source reports, query-plan identity, canonical references, evidence classifications, budgets, and materialization receipts.

### Context pack

A filesystem, MCP, Markdown, graph, JSONL, or Code Mode projection of a compiled context. Packs can be recreated and deleted independently of canonical stores.

## Generations

Compiled contexts are immutable. Enrichment creates a child generation:

~~~text
ContextId
  generation 1: initial compile
  generation 2: dependent Axon research attached
  generation 3: later Cortex observation window attached
~~~

A run pins the generation used for each disclosure and claim.

## Canonical authority

- Axon source documents, manifests, and jobs remain canonical in their stores.
- Cortex observations remain canonical in Cortex-compatible SQLite/artifact stores.
- Graph relationships are rebuildable evidence projections.
- Compiled contexts store references, excerpts, classifications, and receipts.
- Materializations are rebuildable except portable or forensic packs explicitly retained as artifacts.

## Query plan

A compile plan fuses authorized lanes:

~~~text
structured SQL
FTS5
vector retrieval
bounded graph traversal
memory
source generation lookup
observation timelines
~~~

Fusion, reranking, hydration, and truncation are deterministic against pinned inputs where model synthesis is absent.

## Evidence

Each selected context item carries:

- canonical reference;
- evidence class;
- authority and trust;
- sensitivity;
- freshness;
- time and revision;
- entity links;
- content or artifact digest;
- selected excerpt or summary.

Conflicts are first-class and survive compilation.

## Materialization

Materialization creates a receipt and never changes the selected context set. A later request for extra raw records is a disclosure/materialization operation and may create a child context if it changes the reproducible evidence set.

## Retention

Compiled-context metadata should outlive temporary filesystem projections. Source retention can invalidate references; the context then reports expired evidence rather than silently forgetting it existed.
