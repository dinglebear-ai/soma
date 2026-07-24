# Soma Context Layer v1 Master Specification

## 1. Product outcome

Soma v1 becomes one authenticated context application combining:

- the existing Soma MCP gateway and provider runtime;
- Axon-derived knowledge acquisition and RAG;
- Cortex-derived operational observations;
- evidence-backed graph retrieval;
- durable memory;
- the existing Soma CLI, REST API, MCP endpoint, and Aurora web application.

A caller can identify a project, service, repository, host, incident, agent session, or tool and retrieve:

1. exact canonical records;
2. lexical matches;
3. semantic matches;
4. related graph entities and evidence paths;
5. version- and time-aware source code, configuration, and documentation;
6. a grounded synthesis with citations and explicit uncertainty.

## 2. Architectural thesis

Soma MUST NOT force Axon and Cortex into one raw ingestion lifecycle.

Axon-style sources are finite or refreshable and use:

```text
resolve -> discover -> manifest diff -> acquire -> normalize
-> prepare -> index -> publish generation
```

Cortex-style sources are continuous or periodic and use:

```text
receive/collect -> normalize -> persist -> checkpoint
-> enrich -> retain -> project
```

They converge at:

```text
CanonicalRef
Citation
IndexDocument
EvidenceRef
GraphCandidate
MemoryCandidate
```

## 3. Authority model

| Data | Authority |
|---|---|
| Source manifests, documents, observations, sessions, configurations | Canonical SQLite rows or durable artifacts |
| FTS indexes | Rebuildable lexical projection |
| Qdrant points | Rebuildable semantic projection |
| Graph entities, relationships, communities | Rebuildable evidence projection |
| Memory | Durable curated record with evidence and lifecycle |
| Generated synthesis | Ephemeral result, never canonical truth |

No Qdrant point MAY be the sole copy of an observation or document.

## 4. Required v1 capabilities

### Knowledge

- local files and directories;
- Git repositories;
- web pages and crawls;
- GitHub issues, PRs, releases, and repositories;
- package registries supported by Axon;
- YouTube transcripts;
- Reddit;
- AI sessions;
- Markdown, code, structured, transcript, and prose chunk routing;
- TEI embeddings;
- Qdrant dense and sparse indexing;
- FTS5 indexing;
- retrieval, reranking, citations, and optional synthesis;
- source generations, cleanup debt, and durable jobs.

### Observations

- UDP/TCP syslog;
- managed file tails;
- Docker logs and lifecycle events;
- OTLP logs;
- heartbeat snapshots;
- inventory and configuration snapshots;
- shell history;
- AI-session operational evidence;
- canonical SQLite + FTS5 storage;
- retention and storage-budget controls;
- semantic projection outbox.

### Graph and context

- stable entities and aliases;
- deterministic and parser-derived relationships;
- evidence references to canonical records;
- authority, trust, confidence, and temporal validity;
- local entity GraphRAG;
- temporal investigation GraphRAG;
- hybrid FTS + dense + sparse retrieval;
- context-budgeted, cited synthesis;
- graph community schema and interfaces, with full global community search deferred unless the core slices finish early.

### Surfaces

All use cases MUST be exposed through Soma's established application layer. CLI, API, MCP, and Web MUST remain thin projections and MUST NOT implement independent routing or retrieval logic.

## 5. New shared crates

V1 defines sixteen coarse shared crates:

1. `soma-primitives`
2. `soma-sanitize`
3. `soma-process`
4. `soma-route`
5. `soma-sources`
6. `soma-crawl`
7. `soma-ledger`
8. `soma-jobs`
9. `soma-llm`
10. `soma-rag`
11. `soma-transcript`
12. `soma-memory`
13. `soma-observations`
14. `soma-ingest`
15. `soma-collectors`
16. `soma-graph`

Names follow the fixed `soma-<one-word>` convention and MUST receive a crates.io availability check before scaffolding or publication.

## 6. Existing Soma components treated as final

V1 MUST integrate with, not replace:

- authentication and OAuth;
- gateway and upstream MCP management;
- provider-core and provider-adapters;
- Code Mode;
- CLI core;
- HTTP API/server;
- MCP transports;
- observability and traces;
- OpenAPI generation;
- web shell and Aurora components;
- self-update;
- application/domain/runtime/surface layering.

## 7. Product composition

Reusable crates contain mechanisms. Soma product crates contain policy.

V1 SHOULD initially extend the existing product crates by modules rather than create a second constellation of product crates:

```text
crates/soma/domain/src/
  knowledge/
  observations/
  context/
  graph/
  memory/

crates/soma/application/src/
  sources/
  observations/
  context/
  graph/
  memory/
  jobs/

crates/soma/runtime/src/
  knowledge/
  observations/
  storage/
  projection/
  graph/
```

A module becomes a new product crate only after dependency pressure proves the boundary.

## 8. Storage

```text
/var/lib/soma/
  control.db
  knowledge.db
  observations.db
  graph.db
  memory.db
  artifacts/
  sources/
  qdrant/
```

Required Qdrant collections:

```text
soma_knowledge_v1
soma_observations_v1
soma_memory_v1
soma_graph_v1
```

The exact physical split MAY evolve. Logical ownership and rebuild semantics MUST remain stable.

## 9. Semantic projection

Cortex-derived observations MUST be stored first and embedded selectively.

Required initial semantic classes:

- AI sessions and meaningful turns;
- agent/tool/MCP execution segments;
- errors and critical logs;
- stack traces;
- deployment and configuration changes;
- Docker crash/restart/health transitions;
- incident windows;
- inventory changes.

Routine heartbeats, ordinary metrics, repetitive debug logs, and high-volume DNS events MUST NOT be embedded by default.

SQLite writes and projection-outbox writes MUST occur in one transaction. Projection workers MUST use idempotent deterministic point IDs.

## 10. GraphRAG

V1's context broker MUST support:

- direct structured lookup;
- FTS5 retrieval;
- dense/sparse Qdrant retrieval;
- entity resolution;
- bounded graph expansion;
- temporal joins;
- evidence hydration;
- reranking;
- cited synthesis.

Answers MUST distinguish:

- `observed`;
- `documented`;
- `implemented`;
- `historical`;
- `inferred`;
- `unknown`.

## 11. Contract generation

Axon's contract machinery is ported into Soma's existing `xtask`, not published as a separate v1 runtime crate.

Required commands:

```text
cargo xtask context-contracts generate
cargo xtask context-contracts check
cargo xtask context-contracts fixtures
cargo xtask context-contracts dependency-graph
cargo xtask context-contracts donor-parity
```

Generated artifacts include:

- combined JSON Schema bundle;
- database contract;
- vector payload contract;
- crate dependency graph;
- adapter capability matrix;
- observation source matrix;
- events and errors;
- examples and fixture reports.

## 12. Delivery model

Implementation proceeds in vertical slices:

1. contract landing zone;
2. shared foundations;
3. local knowledge walking skeleton;
4. durable jobs;
5. AI sessions;
6. crawler and web;
7. remaining Axon adapters;
8. observation store plus syslog/file tail;
9. Docker, OTLP, heartbeat, inventory, shell history, and configuration snapshots;
10. semantic observation projection;
11. evidence graph and entity resolution;
12. context broker and GraphRAG;
13. memory;
14. hardening, parity, migration, and cutover.

Every slice MUST produce a usable Soma capability. No phase is measured only by lines moved or crates created.

## 13. V1 exit criteria

V1 is complete when the Labby OAuth north-star scenario can:

- resolve Labby and its environment from the project identity;
- retrieve deployed source/configuration and relevant operational evidence;
- retrieve official docs and version-matched code documentation;
- produce an evidence-backed diagnosis and actionable plan;
- cite canonical source documents and observation records;
- expose the same result through CLI, API, MCP, and Web;
- rebuild FTS, vectors, and graph from canonical data;
- pass performance, retention, security, and parity gates.

V1 stops before autonomous implementation or deployment.
