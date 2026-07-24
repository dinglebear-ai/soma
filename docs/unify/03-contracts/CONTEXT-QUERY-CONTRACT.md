# Context Query Contract

## Purpose

The Soma context broker gives CLI, API, MCP, and web callers one query surface over SQL, FTS5, Qdrant, graph, and memory.

The broker is product composition, not a mandatory dependency of every shared crate.

## Modes

- `auto`: planner selects lanes.
- `basic_hybrid`: FTS + dense/sparse retrieval, optional structured filters.
- `entity_local`: resolve seed entities, expand graph, hydrate evidence.
- `temporal_investigation`: bind an entity/incident to a time window and correlate state changes and observations.
- `global_community`: optional v1 stretch mode over community reports.

## Query planning

The planner considers:

- exact identifiers;
- project/service/host/domain names;
- explicit time ranges;
- source and sensitivity scopes;
- lexical versus semantic wording;
- known incident/run/session identities;
- requested output budget;
- caller authorization.

A plan records selected lanes and parameters for observability and evaluation.

## Retrieval lanes

```text
Structured SQL:
    exact filters, time, severity, IDs, counts, current/deployed state

FTS5:
    error strings, stack traces, paths, symbols, commands, exact terminology

Dense/sparse Qdrant:
    conceptual similarity, related incidents, differently worded documentation

Graph:
    topology, dependencies, provenance, temporal relationships, evidence paths

Memory:
    verified decisions, preferences, prior resolutions and procedural knowledge
```

## Evidence hydration

Derived matches MUST be hydrated from canonical records before final citation whenever canonical content remains available.

A vector payload is a pointer and retrieval aid, not sufficient proof by itself.

## Context bundle

A bundle contains:

- selected context items;
- citations;
- entity summaries;
- evidence paths;
- optional timeline;
- score components;
- selected query strategy;
- truncation report;
- optional synthesized answer and classified claims.

## Claim classes

Synthesis SHOULD label claims as:

- `observed`;
- `documented`;
- `implemented`;
- `historical`;
- `inferred`;
- `unknown`.

An inferred claim cites all supporting evidence and includes confidence. Unknowns remain explicit.

## Budgeting

The broker enforces:

- maximum items;
- maximum bytes;
- maximum model tokens when synthesis is enabled;
- per-source and per-kind diversity limits;
- evidence-path depth;
- bounded excerpts.

Raw large result sets remain in durable artifacts or Code Mode-like computation contexts in later versions, but v1 query responses remain bounded.

## Security

The caller's authorization filters every lane before fusion. A graph edge cannot reveal a protected canonical record merely because the entity is visible.

The result's sensitivity is at least the maximum sensitivity of included evidence.

## Determinism

Without synthesis, repeated queries against the same store snapshots, query plan, and backend versions SHOULD return deterministic ordering after score tie-breakers.

## Required E2E scenarios

- exact source-code symbol query;
- semantic documentation query;
- entity-local project query;
- temporal service incident query;
- AI-session history query;
- mixed documentation + logs + configuration investigation;
- unauthorized source exclusion;
- dead citation prevention.
