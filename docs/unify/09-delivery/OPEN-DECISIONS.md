# Open Decisions

These decisions are intentionally unresolved and have owners before implementation reaches the affected slice.

| ID | Decision | Needed by |
|---|---|---|
| O-001 | Crates.io availability and one-word fallback names for any collisions | Crate scaffolding |
| O-002 | Per-crate MSRV and edition policy | Crate scaffolding |
| O-003 | Physical SQLite database split versus logical schemas in fewer files | Observation/knowledge storage |
| O-004 | Exact FTS tokenizer/content-table strategy | Local knowledge and observations |
| O-005 | TEI model and embedding dimensions for v1 | Local knowledge |
| O-006 | Sparse representation, BM42 implementation and collection schema | RAG index |
| O-007 | Reranker provider/model and fallback | Context broker |
| O-008 | Exact Qdrant collection/version switch strategy | RAG index |
| O-009 | Concrete Soma graph vocabulary and relationship enum/profile | Graph slice |
| O-010 | Community detection/global GraphRAG in v1 or post-v1 | GraphRAG slice |
| O-011 | Observation semantic projection policies and thresholds | Projection slice |
| O-012 | Initial retention classes and storage budgets | Observation slice |
| O-013 | Source credential storage/reference shape | Adapter slice |
| O-014 | Spider/Chrome process and network sandboxing inside appliance | Crawl slice |
| O-015 | Which Axon source adapter capabilities are mandatory for v1 | Adapter slice |
| O-016 | How much donor job/history data to migrate | Migration |
| O-017 | Memory candidate creation policy and review defaults | Memory slice |
| O-018 | Graph claim/confidence formulas and policy versioning | Graph slice |
| O-019 | Saved-query/evaluation corpus ownership and licensing | Evaluation |
| O-020 | Release versioning strategy for coordinated crate families | Publication |

An open decision MUST NOT be resolved implicitly in implementation code. It receives an ADR/spec update.
