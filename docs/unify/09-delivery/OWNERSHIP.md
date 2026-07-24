# Suggested Workstream Ownership

Named people are intentionally omitted. Assign one accountable owner per role.

| Workstream | Owns |
|---|---|
| Program/architecture | scope, capability ledger, dependency graph, ADRs |
| Contract | schemas, stable IDs, diagnostics, generated references |
| Knowledge acquisition | route, adapters, crawl |
| RAG | preparation, chunking, embedding, vector, retrieval, citations |
| Runtime | jobs, process, lifecycle, backpressure |
| Observations | canonical model, ingest, receivers/collectors, retention |
| Graph | vocabulary, resolver, evidence, temporal query, projection |
| Memory | lifecycle, store, recall, review |
| Product/application | context broker, use cases, authorization composition |
| Web | Aurora context product surface |
| Operations | packaging, backup, migrations, health, performance |
| Security | threat model, redaction, authz, crawler/provider boundaries |
| Evaluation | donor parity, E2E, GraphRAG and north-star corpus |
| Release | package gates, semver, crates.io publication |

## Architectural review

Changes to the following require cross-workstream review:

- `soma-primitives`;
- stable IDs;
- canonical/derived authority;
- citation contract;
- graph vocabulary/evidence;
- sensitivity/authorization;
- source publication;
- observation acknowledgement/checkpoints;
- public crate feature defaults.
