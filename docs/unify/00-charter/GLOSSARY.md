# Glossary

| Term | Definition |
|---|---|
| Source | User-addressable finite or refreshable thing, such as a repo, site, package, or session archive |
| Source item | Smallest ledger-tracked unit, such as a file, page, issue, package version, or transcript |
| Generation | Publishable snapshot of a mutable source |
| Observation | Time-oriented canonical event, measurement, state snapshot, or execution record |
| Canonical record | Authoritative SQLite row or durable artifact |
| Projection | Rebuildable representation derived from canonical records |
| SourceDocument | Normalized document emitted by a source adapter |
| IndexDocument | Product-neutral semantic input to the RAG pipeline |
| PreparedChunk | Bounded citable unit ready for embedding or lexical indexing |
| Evidence | Canonical reference supporting or contradicting an entity, relationship, claim, or answer |
| Entity | Stable identifiable thing in the graph |
| Relationship | Typed, evidence-backed connection between entities |
| Claim | Potentially disputed or time-bound statement |
| Context broker | Soma product service planning SQL, FTS, vectors, graph, memory, and synthesis |
| Semantic projection | Selective transformation of canonical records into index documents and Qdrant points |
| Cleanup debt | Durable idempotent record of derived data that must be removed or rebuilt |
| Authority | Source quality class, such as deterministic, official, parser-derived, model-derived, or unknown |
| Trust | Policy assessment of how strongly an observation may influence graph confidence |
| Citation | Stable pointer from a result to canonical source material |
