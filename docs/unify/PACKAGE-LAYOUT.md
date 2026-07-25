# Documentation Package Layout

**Package:** `soma-context-v1-documentation-package`  
**Generated:** 2026-07-21  
**Purpose:** Implementation source of truth for merging Axon knowledge and Cortex observations into Soma v1.

## Directory responsibilities

| Directory | Responsibility |
|---|---|
| `00-charter` | Product outcome, v1 boundary, explicit non-goals, glossary and donor baselines |
| `01-architecture` | Target topology, data flow, dependencies, storage, security, context broker and GraphRAG |
| `02-crates` | Full 16-crate catalog, dependency rules, publication rules and per-crate specs |
| `03-contracts` | Normative schemas, semantics, examples, state machines, database and vector contracts |
| `04-product` | Soma application use cases and existing CLI/API/MCP/Web integration |
| `05-migration` | Donor disposition, vertical slices, roadmap, parity, data migration and cutover |
| `06-testing` | Contract, E2E, GraphRAG, scale, security and north-star evaluation |
| `07-operations` | Runtime, backup, retention, reindex, health, upgrade and observability |
| `08-adr` | Architectural decisions that keep v1 aligned |
| `09-delivery` | Readiness/done gates, PR train, risks, ownership and open decisions |

## Complete tree

```text
soma-context-v1-documentation-package/
├── 00-charter
│   ├── GLOSSARY.md
│   ├── NON-GOALS.md
│   ├── PRODUCT-OUTCOME.md
│   ├── SOURCE-BASELINE.md
│   └── V1-SCOPE.md
├── 01-architecture
│   ├── CONTEXT-BROKER.md
│   ├── DATA-FLOW.md
│   ├── DEPENDENCY-LAYERS.md
│   ├── GRAPHRAG.md
│   ├── REPOSITORY-LAYOUT.md
│   ├── SECURITY-BOUNDARIES.md
│   ├── STORAGE-ARCHITECTURE.md
│   └── TARGET-ARCHITECTURE.md
├── 02-crates
│   ├── specs
│   │   ├── soma-transcript.md
│   │   ├── soma-crawl.md
│   │   ├── soma-graph.md
│   │   ├── soma-jobs.md
│   │   ├── soma-llm.md
│   │   ├── soma-memory.md
│   │   ├── soma-collectors.md
│   │   ├── soma-observations.md
│   │   ├── soma-ingest.md
│   │   ├── soma-primitives.md
│   │   ├── soma-process.md
│   │   ├── soma-rag.md
│   │   ├── soma-sanitize.md
│   │   ├── soma-sources.md
│   │   ├── soma-ledger.md
│   │   └── soma-route.md
│   ├── CATALOG.md
│   ├── crates.yaml
│   ├── DEPENDENCY-GRAPH.md
│   ├── EXISTING-SOMA-FOUNDATION.md
│   └── PUBLICATION-RULES.md
├── 03-contracts
│   ├── examples
│   │   ├── context-query.json
│   │   ├── graph-candidate.json
│   │   ├── observation-record.json
│   │   └── source-request.json
│   ├── CITATION-CONTRACT.md
│   ├── CONTEXT-QUERY-CONTRACT.md
│   ├── DATABASE-CONTRACT.md
│   ├── ERROR-CONTRACT.md
│   ├── EVENT-CONTRACT.md
│   ├── GRAPH-CONTRACT.md
│   ├── OBSERVATION-CONTRACT.md
│   ├── RAG-CONTRACT.md
│   ├── README.md
│   ├── REDACTION-CONTRACT.md
│   ├── RETENTION-CONTRACT.md
│   ├── schemas.json
│   ├── SOURCE-CONTRACT.md
│   ├── STATE-MACHINES.md
│   └── VECTOR-PAYLOAD-CONTRACT.md
├── 04-product
│   ├── APPLICATION-USE-CASES.md
│   ├── AUTHORIZATION.md
│   ├── CONFIGURATION.md
│   ├── JOBS-AND-PROGRESS.md
│   ├── SURFACE-INTEGRATION.md
│   └── WEB-SURFACE.md
├── 05-migration
│   ├── AXON-EXTRACTION.md
│   ├── capability-matrix.yaml
│   ├── CORTEX-EXTRACTION.md
│   ├── CUTOVER-PLAN.md
│   ├── DATA-MIGRATION.md
│   ├── DONOR-CODE-DISPOSITION.md
│   ├── donor-path-map.yaml
│   ├── donors.lock.example.toml
│   ├── IMPLEMENTATION-ROADMAP.md
│   ├── PARITY-PLAN.md
│   ├── status.yaml
│   └── VERTICAL-SLICES.md
├── 06-testing
│   ├── CONTRACT-TESTS.md
│   ├── E2E-SCENARIOS.md
│   ├── GRAPHRAG-EVALUATION.md
│   ├── NORTH-STAR-LABBY-OAUTH.md
│   ├── PERFORMANCE-AND-SCALE.md
│   ├── SECURITY-TESTING.md
│   └── TEST-STRATEGY.md
├── 07-operations
│   ├── BACKUP-RESTORE.md
│   ├── HEALTH-AND-RECOVERY.md
│   ├── OBSERVABILITY.md
│   ├── RETENTION-AND-REINDEX.md
│   ├── RUNTIME-TOPOLOGY.md
│   └── UPGRADE-AND-MIGRATIONS.md
├── 08-adr
│   ├── 0001-v1-scope.md
│   ├── 0002-multiple-ingestion-protocols.md
│   ├── 0003-storage-authority.md
│   ├── 0004-selective-observation-vectorization.md
│   ├── 0005-coarse-shared-crates.md
│   ├── 0006-contract-machinery.md
│   ├── 0007-context-broker-product-layer.md
│   ├── 0008-graph-sqlite.md
│   ├── 0009-ai-session-model.md
│   ├── 0010-existing-soma-surfaces.md
│   ├── 0011-semantic-outbox.md
│   └── README.md
├── 09-delivery
│   ├── DEFINITION-OF-DONE.md
│   ├── DEFINITION-OF-READY.md
│   ├── IMPLEMENTATION-TRACKER.md
│   ├── OPEN-DECISIONS.md
│   ├── OWNERSHIP.md
│   ├── PR-TRAIN.md
│   ├── RISK-REGISTER.md
│   └── risk-register.yaml
├── START-HERE.md
├── CHANGELOG.md
├── CHECKSUMS.sha256
├── MANIFEST.yaml
├── MASTER-SPEC.md
├── PACKAGE-LAYOUT.md
├── README.md
└── VALIDATION-REPORT.md
```

## Canonical entry points

- `START-HERE.md`
- `README.md`
- `MASTER-SPEC.md`
- `02-crates/CATALOG.md`
- `03-contracts/README.md`
- `05-migration/IMPLEMENTATION-ROADMAP.md`
- `06-testing/NORTH-STAR-LABBY-OAUTH.md`

## Machine-readable sources of truth

- `02-crates/crates.yaml`
- `03-contracts/schemas.json`
- `05-migration/capability-matrix.yaml`
- `05-migration/donor-path-map.yaml`
- `05-migration/status.yaml`
- `09-delivery/risk-register.yaml`
- `MANIFEST.yaml`
