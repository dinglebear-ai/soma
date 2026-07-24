# Start Here

**Purpose:** Give implementors the shortest reliable path from this documentation package to the first working Soma context capability.

## The plan in one sentence

Finish Soma's existing gateway and Aurora-based product shell, then transplant Axon's refreshable knowledge pipeline and Cortex's continuous observation pipeline as reusable crates, keep their ingestion lifecycles separate, and join them through one evidence-backed context plane built on SQLite, FTS5, Qdrant, graph relationships, memory, and Soma's existing CLI, API, MCP, and web surfaces.

## V1 boundary

### Already authoritative in Soma

The following are treated as the final product direction and are **not** reopened by this program:

- authentication and OAuth;
- the MCP gateway and upstream-server composition;
- the provider catalog and Code Mode;
- CLI, REST API, MCP, OpenAPI, and web-surface projection;
- the Aurora-based web application shell;
- observability and MCP traces;
- transactional self-update behavior;
- the existing Soma application, runtime, and composition architecture.

Axon and Cortex contribute context capabilities to that chassis. They do not redefine it.

### In scope

V1 adds:

- Axon-derived source routing, adapters, crawling, generations, chunking, embeddings, vector indexing, retrieval, reranking, synthesis, AI transcripts, jobs, graph candidates, and memory;
- Cortex-derived logs, events, telemetry, file tailing, syslog, Docker, OTLP, heartbeats, inventory, shell history, retention, temporal evidence, and operational correlation;
- canonical SQLite storage and FTS5 search;
- selective semantic projection into Qdrant;
- one evidence-first graph connecting knowledge, infrastructure, configurations, sessions, tools, and observations;
- one Soma context broker that can combine exact records, lexical search, vectors, graph traversal, and memory into cited answers.

### Explicitly out of scope

V1 does **not** include:

- Agent Package Manager or `apm.yaml` / `apm.lock`;
- the Orchestrator agent or implementation workers;
- agent deployment into Incus containers;
- custom Incus image construction;
- autonomous pull requests, merges, deployments, or remediation;
- chat-service bridges;
- self-improving skills, tools, prompts, or agents.

AI-session and agent-activity ingestion remain in scope because they are context inputs. Agent orchestration does not.

## The architecture that must not be blurred

Soma does **not** force Axon and Cortex through one universal ingestion lifecycle.

```text
Refreshable knowledge                         Continuing observations
files / repos / web / sessions                logs / events / telemetry
          |                                              |
          v                                              v
route → manifest → diff → generation       receive → batch → checkpoint → retain
          |                                              |
          v                                              v
     SourceDocument                              ObservationRecord
          \                                              /
           \                                            /
            +---- citations, evidence and projections --+
                                |
                    SQLite + FTS5 + Qdrant + Graph
                                |
                         Soma Context Broker
                                |
                       CLI / API / MCP / Web
```

The unification point is the downstream context plane:

- `CanonicalRef` identifies authoritative records;
- `Citation` makes answers inspectable;
- `IndexDocument` feeds semantic processing;
- `EvidenceRef` grounds graph relationships and claims;
- `GraphCandidate` joins knowledge and operational reality;
- `MemoryCandidate` allows verified lessons to become durable memory.

**Separate ingestion protocols, shared context projections, one retrieval plane.**

## Storage authority

The storage roles are fixed:

```text
SQLite and durable artifacts  = canonical truth
FTS5                           = exact lexical search
Qdrant                         = rebuildable semantic search
Evidence graph                 = rebuildable relationships and correlation
Memory                         = curated, evidence-backed durable knowledge
LLM synthesis                  = ephemeral output, never canonical truth
```

Every Cortex observation remains canonical in SQLite. Only useful semantic units, such as incident windows, errors, deployment changes, agent-run segments, and novel event clusters, are projected into Qdrant. Raw heartbeats and repetitive log lines are not embedded merely because they exist.

## Proposed shared crates

The target catalog contains 16 coarse, publishable crates:

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

These are the **target catalog**, not a command to scaffold sixteen empty packages immediately.

## What to create first

### Slice 1: Contract landing zone

Before transplanting implementation code:

1. Pin full donor commits for Soma, Axon, and Cortex.
2. Port Axon's contract-generation behavior into Soma's existing `xtask`.
3. Establish the shared schema bundle, fixture registry, donor-path map, and dependency-layer checks.
4. Make contract and architecture drift fail CI.

Read:

- [`03-contracts/README.md`](03-contracts/README.md)
- [`05-migration/DONOR-CODE-DISPOSITION.md`](05-migration/DONOR-CODE-DISPOSITION.md)
- [`09-delivery/DEFINITION-OF-READY.md`](09-delivery/DEFINITION-OF-READY.md)

### Slice 2: Shared foundations

Implement only the foundations required by the first real capability:

- `soma-primitives`;
- `soma-sanitize`;
- `soma-process` when process-backed acquisition or providers require it.

Keep `soma-primitives` microscopic. A type belongs there only when multiple independent shared domains must exchange it.

### Slice 3: Local knowledge walking skeleton

The first product milestone is not “extract the Axon pipeline.” It is this complete, demonstrable behavior:

```text
Local repository
    ↓
route and register source
    ↓
discover files and build manifest
    ↓
diff and create generation
    ↓
normalize SourceDocuments
    ↓
select Markdown, code, or prose chunking
    ↓
store canonical records and FTS5 index
    ↓
embed and upsert into Qdrant
    ↓
publish generation
    ↓
return cited search results through Soma
```

The same application use case must be reachable through the existing:

- web application;
- CLI;
- REST API;
- MCP endpoint.

A concrete acceptance test is:

```bash
soma source add ./path/to/repository
soma context search "Where is OAuth configured?"
```

The answer must contain exact citations that hydrate back to canonical source records.

This slice will likely require:

- `soma-primitives`;
- `soma-sanitize`;
- `soma-route`;
- `soma-sources` with only the local-source feature initially;
- `soma-ledger`;
- `soma-rag`;
- `soma-llm` only when synthesis is enabled;
- Soma product-specific knowledge composition and context-query use cases.

Do not add GitHub, Reddit, YouTube, registries, web crawling, memory, or Cortex collectors before this artery works end to end.

## Implementation order

Follow the vertical slices in order:

1. Complete the existing gateway and Aurora product shell.
2. Land executable contracts, donor baselines, fixtures, and architecture checks.
3. Implement shared foundations.
4. Deliver local knowledge ingestion and cited retrieval end to end.
5. Add durable jobs around the already-working synchronous pipeline.
6. Unify Claude, Codex, and Gemini transcript parsing and projections.
7. Add Spider/Chrome crawling and web sources.
8. Add the remaining Axon source families one at a time.
9. Add Cortex's canonical observation store, reliable ingestion, file tailing, and syslog.
10. Add Docker, OTLP, heartbeat, inventory, configuration, shell-history, and session collectors.
11. Selectively project valuable observations through the shared RAG engine.
12. Join both domains through the evidence-first graph.
13. Build the product-specific context broker and GraphRAG query modes.
14. Add evidence-backed memory.
15. Complete migration, parity, performance, security, backup, reindex, and cutover gates.

Maintain a work-in-progress limit of one active vertical product slice plus one narrow foundation or publication task.

## Five rules that must never be violated

### 1. Deliver capabilities, not a crate collection

**Do not create all proposed crates before delivering the first end-to-end Soma capability.**

A crate is complete only when Soma uses it in a real slice, donor behavior is verified, and an independent consumer can use the packaged crate outside the workspace.

### 2. Preserve the two ingestion semantics

Knowledge sources use manifests and committed generations. Operational observations use streams, batches, checkpoints, and retention. Neither model may infect the other merely to make a diagram look simpler.

### 3. Keep canonical truth separate from projections

SQLite and durable artifacts remain authoritative. FTS5, Qdrant, graph summaries, and synthesized answers must be rebuildable and traceable to canonical evidence.

### 4. Keep shared crates product-neutral

No shared crate may depend on `crates/soma/*`, `apps/*`, Soma authorization, Soma surface DTOs, product environment variables, or product-specific runtime policy. Concrete composition belongs in Soma's application/runtime layer and `apps/soma`.

### 5. Require citations and evidence at every cross-domain boundary

A vector match, graph edge, memory, diagnosis, or synthesized answer must resolve to canonical source content or SQLite records. “The model inferred it” is not sufficient provenance.

## How progress is measured

Do not measure the program by lines moved or packages created. Measure:

- end-to-end capabilities Soma owns;
- donor contracts and fixtures preserved;
- duplicate implementations eliminated;
- shared crates independently usable;
- cited answers available through all existing surfaces;
- knowledge and observations correlated through evidence;
- donor capabilities safely retired.

## Immediate next actions

1. Finish the gateway and full-product web shell to the desired baseline.
2. Replace abbreviated donor SHAs in [`05-migration/donors.lock.example.toml`](05-migration/donors.lock.example.toml) with full pinned commits.
3. Resolve the readiness items in [`09-delivery/OPEN-DECISIONS.md`](09-delivery/OPEN-DECISIONS.md) that block Slice 1.
4. Implement the `xtask` contract landing zone.
5. Begin the local-knowledge PR train with contracts and fixtures, not copied implementation code.

## Read next

1. [`MASTER-SPEC.md`](MASTER-SPEC.md) for the complete normative product specification.
2. [`00-charter/V1-SCOPE.md`](00-charter/V1-SCOPE.md) for the hard v1 boundary.
3. [`01-architecture/TARGET-ARCHITECTURE.md`](01-architecture/TARGET-ARCHITECTURE.md) and [`01-architecture/DATA-FLOW.md`](01-architecture/DATA-FLOW.md) for system shape.
4. [`02-crates/CATALOG.md`](02-crates/CATALOG.md) and [`02-crates/DEPENDENCY-GRAPH.md`](02-crates/DEPENDENCY-GRAPH.md) for package ownership and dependency rules.
5. [`05-migration/VERTICAL-SLICES.md`](05-migration/VERTICAL-SLICES.md) and [`05-migration/IMPLEMENTATION-ROADMAP.md`](05-migration/IMPLEMENTATION-ROADMAP.md) for delivery order.
6. [`06-testing/NORTH-STAR-LABBY-OAUTH.md`](06-testing/NORTH-STAR-LABBY-OAUTH.md) for the outcome the full context layer must eventually prove.

The north star is straightforward: Soma should be able to combine what is deployed, what happened, what the source implements, what the official documentation requires, and what previous work learned into one cited, evidence-backed technical answer. Everything in v1 exists to make that possible.
