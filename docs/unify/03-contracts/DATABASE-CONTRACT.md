# Database Ownership Contract

## Logical databases

```text
control.db
    sources, generations, jobs, provider/product configuration

knowledge.db
    canonical documents/chunks metadata, FTS, source lookup

observations.db
    canonical observations, FTS, checkpoints, semantic outbox

graph.db
    entities, aliases, relationships, claims, evidence, projections

memory.db
    memory records, lifecycle, reinforcement, review
```

Physical consolidation is allowed only behind the same logical ownership boundaries.

## SQLite requirements

- WAL mode for operational databases unless the deployment environment proves another mode superior.
- Busy timeout and bounded retry.
- Foreign keys enabled.
- Explicit migrations with checksums.
- Separate read connections/pool from serialized write coordination where appropriate.
- Prepared statements and bounded query results.
- No untrusted dynamic SQL identifiers.
- FTS external-content or synchronization strategy documented and tested.
- Database integrity and migration checks exposed through `doctor`.

## Transaction boundaries

### Observation write

One transaction commits:

- canonical rows;
- FTS synchronization;
- semantic outbox tasks;
- checkpoint intent when store model permits.

### Source publication

The ledger transaction commits the authoritative generation pointer and cleanup debt. Qdrant cannot participate in the transaction and remains staged/rebuildable.

### Graph projection

A projection batch commits entities/relationships/evidence plus projection version atomically where feasible.

## Schema ownership

Each shared storage adapter owns its migrations. Soma product migrations may compose them but MUST NOT duplicate table definitions.

## Backup

Backups capture a consistent SQLite snapshot and record Qdrant rebuild metadata. Canonical SQLite/artifacts are higher priority than derived vector snapshots.

## Query safety

All product queries apply authorization filters before returning records. FTS and graph queries have explicit row, time, depth, and byte limits.
