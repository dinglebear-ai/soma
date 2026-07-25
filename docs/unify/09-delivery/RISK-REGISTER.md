# Risk Register

| ID | Risk | Impact | Mitigation | Owner |
|---|---|---|---|---|
| R-001 | Axon size and hidden live orchestration | High | Trace actual call paths and start with local vertical slice; do not copy crates mechanically. | Architecture owner |
| R-002 | Duplicate canonical types emerge | High | primitives/domain ownership registry and xtask duplicate-type checks. | Contract owner |
| R-003 | soma-rag becomes an internal monolith | High | Internal modules, narrow traits, optional features, public API review, compile-time feature matrix. | RAG owner |
| R-004 | primitives becomes new axon-api/core | High | Strict line/dependency responsibility review; no storage/providers/product DTOs. | Contract owner |
| R-005 | Jobs retain Axon domain dependencies | High | Extract runtime mechanics only; product runners in Soma. | Runtime owner |
| R-006 | Vectorize too much operational data | High | Selective projection policy, outbox metrics, retrieval value evaluation. | Observation/RAG owners |
| R-007 | Dead citations after retention | High | Retention cleanup debt, promotion contract, dead-evidence checks. | Storage owner |
| R-008 | Entity resolution merges unrelated things | High | Deterministic IDs first, ambiguity preservation, review, reversible projections. | Graph owner |
| R-009 | Current versus deployed state confusion | High | Temporal/versioned graph, artifact/commit/config snapshots, query tests. | Graph/product owners |
| R-010 | Authorization leaks through vector/graph lanes | Critical | Pre-retrieval filtering, sensitivity inheritance, adversarial E2E. | Security owner |
| R-011 | TEI/Qdrant outage blocks logs | Critical | Transactional semantic outbox and degraded-mode tests. | Runtime owner |
| R-012 | Crawler SSRF/scope escape | Critical | URL policy, DNS/redirect checks, network sandbox, security fixtures. | Crawler owner |
| R-013 | Secret leakage from logs/sessions | Critical | sanitize at every boundary, secret corpus, vector/citation scans. | Security owner |
| R-014 | Donor behavior drifts during migration | Medium | Pinned SHAs and explicit rebaseline process. | Migration owner |
| R-015 | Crate publication freezes poor APIs | High | Publish after Soma use, external consumer, API/semver review. | Release owner |
| R-016 | Web product gets blocked on backend churn | Medium | Stable application contracts, mock fixtures, vertical page delivery. | Web owner |
| R-017 | SQLite growth/throughput exceeds appliance | Medium | Budgets, retention, partition option behind store interfaces, benchmarks. | Storage owner |
| R-018 | GraphRAG adds cost without quality | High | Ablation evaluation and basic hybrid fallback. | Context owner |
| R-019 | Scope expands into autonomous agents | High | ADR 0001, CI schema check for forbidden concepts, separate future program. | Program owner |
| R-020 | One long migration branch becomes unmergeable | High | PR train and WIP limit. | Program owner |
