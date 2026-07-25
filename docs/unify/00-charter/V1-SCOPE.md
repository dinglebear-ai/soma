# V1 Scope

## Mission

Create the first integrated Soma context layer by transplanting reusable knowledge mechanisms from Axon and reusable observation mechanisms from Cortex, while preserving Soma's existing gateway, authentication, provider, surface, and web architecture.

## In scope

### Axon-derived

- source identity, routing, manifests, generations, and cleanup debt;
- feature-gated source adapters;
- Spider/Chrome crawling;
- document normalization;
- Markdown, source-code, structured, transcript, and prose chunking;
- parsing and metadata extraction;
- embedding, vector publication, FTS, retrieval, reranking, citations, synthesis;
- durable jobs;
- typed AI-session model;
- memory;
- source-derived graph candidates;
- schema/contract generation ported into Soma `xtask`.

### Cortex-derived

- observation model and canonical store;
- bounded ingest queue, batching, backpressure, retries, shutdown drain;
- syslog, file-tail, Docker, OTLP, heartbeat, inventory, shell-history, configuration, and session sources;
- SQLite/FTS search and retention;
- evidence-first graph projection;
- canonical entity resolution;
- temporal correlation;
- selective semantic projection into the shared RAG pipeline.

### Soma-specific composition

- context broker;
- hybrid and graph-aware query planning;
- knowledge/observation/graph/memory web views;
- application use cases exposed through existing CLI/API/MCP/Web;
- storage bootstrap, migrations, health, backup, rebuild, and upgrade policy.

## Not in scope

- Agent Package Manager;
- mission packages;
- Orchestrator agent;
- worker agents;
- agent monitors;
- Incus worker provisioning;
- custom Incus image compilation;
- autonomous changes, PRs, merges, deployment, or rollback;
- chat service channels;
- self-improving skills/tools;
- replacing Soma auth, gateway, provider catalog, Code Mode, or surface projection;
- replacing SQLite with a distributed log database;
- adding Neo4j or another graph database;
- multi-tenant isolation guarantees.

## Scope guard

A v1 change MUST justify itself against at least one of these outcomes:

1. acquire knowledge;
2. acquire observations;
3. preserve canonical truth;
4. create evidence-backed graph context;
5. retrieve and synthesize context;
6. expose that capability through existing Soma surfaces;
7. operate or verify the above safely.

Anything else is deferred.
