# cortex-storage-sqlite

`cortex-storage-sqlite` is Cortex's reusable SQLite persistence adapter. It
owns connection pooling, schema migrations, transactional writes, query/FTS
projections, retention and storage-budget enforcement, graph persistence, event
and incident persistence, heartbeat persistence, and observatory tables.

The dependency direction is one-way: this crate may depend on
`cortex-domain`, `cortex-ingest-core`, and the pure `cortex-inventory` snapshot
contract; those crates do not depend on SQLite. Product runtime configuration,
transport DTOs, scanner implementations, collectors, and application services
are not storage dependencies. Normalized scanner events cross the boundary via
storage-neutral input contracts.

## Compatibility

The extraction baseline is Cortex commit
`7edf23fadb94650c2d2a2f9c80111fb44319eea8`. Migration ordering,
`KNOWN_SCHEMA_VERSION`, PRAGMA behavior, write-lock coordination, and donor
database fixtures are parity contracts. Raw SQLite row types remain adapter
details unless they are explicitly documented as storage/query projections.

Application-facing persistence capabilities that are already consumed by donor
services are explicit storage ports, including error-signature state, notification
outbox/firings, stream health, LLM invocation persistence, observatory paging,
pattern-row queries, and a closed-enum PRAGMA diagnostics API. Pure graph
confidence math is intentionally owned by `cortex-domain` instead of SQLite.
