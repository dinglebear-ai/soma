# Soma Product Family and Context Layer Documentation Package

**Status:** Proposed implementation source of truth
**Scope:** Build a multi-distribution monorepo while integrating Axon knowledge, Cortex observations, and Synapse operations into Soma
**Audit dates:** Context baseline 2026-07-21; product-family extension 2026-07-31
**Product boundary:** Labby, Axon, Cortex, and Synapse remain complete standalone products; Soma is the integrated superset.

Begin with [`START-HERE.md`](START-HERE.md) for the implementation sequence, first milestone, and non-negotiable guardrails.

This package defines the product-family boundaries and the first integrated Soma context layer:

- five independent composition roots for Labby, Axon, Cortex, Synapse, and Soma;
- independently consumable shared engines with no product policy;
- a distinct Synapse-derived operations plane connected to Cortex through lifecycle events;

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

- **19 planned shared-crate specifications plus existing neutral clients** with ownership, exclusions, APIs, features, dependencies, tests, consumers, and publication gates.
- **One combined JSON Schema bundle** with representative validated fixtures.
- **Source, observation, RAG, graph, query, citation, operation, progress, verification, event, redaction, retention, database, vector, and state-machine contracts.**
- **Axon, Cortex, and Synapse donor disposition maps** with pinned donor commits and parity fixtures.
- **A 14-phase context roadmap plus a seven-phase operations track** and machine-readable capability ledger.
- **A complete product-use-case and Aurora web-surface plan.**
- **Parity, E2E, GraphRAG, operations, performance, security, backup, migration, retention, and cutover plans.**
- **Thirteen ADRs**, a stacked implementation PR train, risk register, definitions of ready/done, and open-decision ledger.
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
05-migration/     Axon/Cortex/Synapse extraction maps, vertical slices, parity, cutover
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
