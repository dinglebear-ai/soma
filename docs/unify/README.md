# Soma Context Layer v1 Documentation Package

**Status:** Proposed implementation source of truth  
**Scope:** Merge Axon's knowledge pipeline and Cortex's observation platform into Soma  
**Audit date:** 2026-07-21  
**Product boundary:** Existing Soma gateway, authentication, provider catalog, Code Mode, CLI/API/MCP projection, and web shell remain authoritative.

Begin with [`START-HERE.md`](START-HERE.md) for the implementation sequence, first milestone, and non-negotiable guardrails.

This package defines the first integrated Soma context layer:

- heterogeneous knowledge ingestion from Axon-derived source adapters;
- operational observation ingestion from Cortex-derived receivers and collectors;
- canonical SQLite + FTS5 storage;
- selective semantic projection into Qdrant;
- one evidence-backed graph connecting knowledge, infrastructure, sessions, tools, and events;
- hybrid and graph-aware retrieval through Soma's existing CLI, API, MCP, and web surfaces;
- durable memory over verified facts and lessons.

## Explicit v1 non-goals

The following are **not part of v1**:

- Agent Package Manager (`apm.yaml` / `apm.lock`);
- Orchestrator or worker-agent workflows;
- dispatching agents into Incus containers;
- custom Incus image construction;
- autonomous PR creation, merging, deployment, or remediation;
- chat-channel bridges;
- self-modifying skills, tools, prompts, or agents.

The schemas reserve no mandatory fields for those systems. They may be layered on later without contaminating v1's reusable crates.


## Package deliverables

- **16 shared-crate specifications** with ownership, exclusions, APIs, features, dependencies, tests, consumers, and publication gates.
- **One combined JSON Schema bundle** with representative validated fixtures.
- **Source, observation, RAG, graph, query, citation, error, event, redaction, retention, database, vector, and state-machine contracts.**
- **Axon and Cortex donor disposition maps** covering every Axon crate and the relevant Cortex subsystems.
- **A 14-phase vertical-slice implementation roadmap** and machine-readable capability ledger.
- **A complete product-use-case and Aurora web-surface plan.**
- **Parity, E2E, GraphRAG, performance, security, backup, migration, retention, and cutover plans.**
- **Eleven v1 ADRs**, an implementation PR train, risk register, definitions of ready/done, and open-decision ledger.
- **The Labby OAuth north-star evaluation scenario**, scoped to evidence-backed diagnosis and remediation planning in v1.

## Reading order

1. [`START-HERE.md`](START-HERE.md)
2. [`MASTER-SPEC.md`](MASTER-SPEC.md)
3. [`PACKAGE-LAYOUT.md`](PACKAGE-LAYOUT.md)
4. [`00-charter/V1-SCOPE.md`](00-charter/V1-SCOPE.md)
5. [`01-architecture/TARGET-ARCHITECTURE.md`](01-architecture/TARGET-ARCHITECTURE.md)
6. [`02-crates/CATALOG.md`](02-crates/CATALOG.md)
7. [`03-contracts/README.md`](03-contracts/README.md)
8. [`05-migration/IMPLEMENTATION-ROADMAP.md`](05-migration/IMPLEMENTATION-ROADMAP.md)
9. [`06-testing/NORTH-STAR-LABBY-OAUTH.md`](06-testing/NORTH-STAR-LABBY-OAUTH.md)
10. [`VALIDATION-REPORT.md`](VALIDATION-REPORT.md)

## Normative language

The words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are normative.

## Package map

```text
00-charter/       Product boundary, goals, non-goals, glossary, donor baselines
01-architecture/  Target topology, data flows, dependencies, storage, GraphRAG
02-crates/        Complete shared-crate catalog and per-crate implementation specs
03-contracts/     Normative runtime, storage, citation, schema, and state contracts
04-product/       Soma application use cases and existing surface integration
05-migration/     Axon/Cortex extraction map, vertical slices, parity, cutover
06-testing/       Unit, contract, E2E, GraphRAG, performance, and security plans
07-operations/    Runtime services, backup, retention, rebuild, health, upgrade
08-adr/           Accepted architectural decisions for v1
09-delivery/      Readiness, done criteria, PR train, risks, open decisions
```

## Canonical implementation principle

Multiple ingestion protocols feed one context plane:

```text
Refreshable knowledge                      Continuing observations
files / repos / web / sessions             logs / OTLP / Docker / telemetry
          |                                            |
          v                                            v
SourceDocument                                ObservationRecord
          \                                            /
           \                                          /
            +--> citations + evidence + projections -+
                              |
                 SQLite + FTS5 + Qdrant + Graph
                              |
                       Context Broker
                              |
                     CLI / API / MCP / Web
```

SQLite remains authoritative. Qdrant and graph summaries are rebuildable projections.
