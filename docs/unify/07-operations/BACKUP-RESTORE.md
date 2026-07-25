# Backup and Restore

## Priority

1. configuration and secret references;
2. canonical SQLite databases;
3. durable artifacts/source captures;
4. encryption/signing material required to restore access;
5. Qdrant snapshots as acceleration, not sole truth;
6. generated caches.

## Consistent backup

A backup records:

- Soma version;
- schema/migration versions;
- database checksums and snapshot method;
- artifact manifest/checksums;
- Qdrant collection schemas, embedding profiles, and optional snapshots;
- TEI model identity;
- enabled source/receiver configuration;
- created time and host identity.

SQLite backups use the online backup API or another consistency-safe mechanism.

## Restore

1. install compatible Soma appliance;
2. stop writers;
3. restore configuration/secrets;
4. restore canonical databases and artifacts;
5. run migration checks;
6. restore Qdrant snapshots or create reindex jobs;
7. verify FTS synchronization;
8. verify graph/memory evidence resolution;
9. start workers/receivers;
10. run saved query and health checks.

## Test

A release backup test destroys an isolated instance, restores it, rebuilds derived indexes, and compares:

- canonical counts/hashes;
- source committed generations;
- observation time ranges;
- saved query required evidence;
- memory records;
- graph paths;
- authorization behavior.

## Retention

Backup retention and encryption are deployment policy. Backups containing canonical observations or browser/source artifacts are treated at the highest included sensitivity.
