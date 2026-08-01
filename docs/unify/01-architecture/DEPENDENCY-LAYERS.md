---
title: "Dependency Layers"
created: 2026-07-24
updated: 2026-07-31
---

# Dependency Layers

## Layer 0: leaf primitives and protocol clients

- `soma-primitives`
- `soma-sanitize`
- `soma-process`
- the neutral Incus client
- other pure integration clients that satisfy shared-crate rules

Constraints:

- no product dependencies;
- no ambient product configuration;
- minimal default features;
- network, database, and runtime integrations are explicit or feature-gated.

## Layer 1: domain protocols

- `soma-route`
- `soma-transcript`
- `soma-observations`
- `soma-llm`
- `soma-graph`
- `soma-ops`

## Layer 2: engines and adapters

- `soma-sources`
- `soma-crawl`
- `soma-ledger`
- `soma-jobs`
- `soma-rag`
- `soma-memory`
- `soma-ingest`
- `soma-collectors`
- `soma-fleet`
- `soma-infra`

## Layer 3: product application composition

`crates/{labby,axon,cortex,synapse,soma}/application` and corresponding domain/runtime crates compose shared engines and define product policy.

Product application crates may define ports consumed by product-owned integrations. A product may not import another product's surface or composition crates as an embedded engine.

## Layer 4: product surfaces

`crates/<product>/{cli,api,mcp,web}` call that product's application use cases only.

## Layer 5: executables

`apps/<product>` owns configuration loading, concrete backend construction, worker lifecycle, routing, signals, and shutdown for exactly one distribution.

## Prohibited edges

- shared -> `crates/<product>/*`;
- shared -> `apps/*`;
- standalone product -> `crates/soma/*` or `apps/soma`;
- observation adapters -> RAG orchestration;
- source adapters -> Qdrant directly;
- jobs -> source adapters/RAG/ledger product runners;
- RAG -> source acquisition;
- graph -> product web/API/MCP;
- product surfaces -> database, Docker, SSH, or Incus clients;
- product integrations -> another product's internal types;
- neutral operation specifications -> Synapse scopes or Soma principals.

## Allowed bridges

- product-owned integrations may implement product application ports over shared engines;
- product-owned remote clients may implement the same ports through stable public contracts;
- apps may select embedded, remote, or disabled implementations;
- operation events may cross into Cortex through the neutral event contract, never through direct database access.
