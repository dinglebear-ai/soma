# Donor Data Migration

## Principles

- Prefer rebuilding derived indexes from canonical sources.
- Preserve stable canonical IDs or record an explicit ID mapping.
- Do not dual-write indefinitely.
- Pin donor database schema and commit before export.
- Verify counts, hashes, time ranges, and random samples.

## Axon

### Migrate

- configured sources and refresh policies;
- current committed source/generation metadata when preserving continuity is valuable;
- durable artifacts not reproducible from origins;
- active reviewed memories;
- operator labels and sensitivity;
- job history only when operationally useful.

### Rebuild

- prepared chunks;
- FTS indexes;
- embeddings;
- Qdrant points;
- graph projections;
- community reports.

Rebuilding guarantees the new Soma processing fingerprint and payload contract.

## Cortex

### Migrate

- canonical observations within retained windows;
- source/checkpoint configuration;
- inventory/configuration snapshots;
- pinned incidents/evidence;
- relevant AI-session and shell-history records;
- retention holds;
- source identity mappings.

### Rebuild

- FTS rows where migration validation favors rebuild;
- semantic projections;
- graph projections;
- derived summaries and clusters.

## Migration tool shape

```text
soma migrate inspect --from axon|cortex
soma migrate plan --from ... --output plan.json
soma migrate execute --plan plan.json
soma migrate verify --plan plan.json
```

The plan is immutable and records:

- donor commit/schema;
- source database checksums;
- time and kind filters;
- ID mapping policy;
- target migration versions;
- rebuild jobs;
- rollback/restore points.

## Verification

- table/record counts by kind and day;
- min/max timestamps;
- source/generation counts;
- content-hash samples;
- FTS sample queries;
- citation resolution;
- graph evidence resolution;
- reindex completion;
- no secret regressions.

## Cutover

1. stop donor writes;
2. take final consistent snapshots;
3. run incremental final import;
4. verify;
5. start Soma receivers/sources;
6. monitor gaps/duplicates;
7. retain donor snapshot until acceptance window closes.
