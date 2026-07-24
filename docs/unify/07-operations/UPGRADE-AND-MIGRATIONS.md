# Upgrade and Migration Operations

## Compatibility dimensions

An upgrade may change:

- product config;
- SQLite schemas;
- source/manifest contracts;
- chunk processing fingerprint;
- embedding/vector schemas;
- graph projectors;
- memory schemas;
- public API/OpenAPI;
- shared crate serialization.

Each release declares which migrations and rebuilds are required.

## Upgrade order

1. create verified backup;
2. validate target binary/artifact;
3. stop or quiesce writers;
4. run preflight and migration plan;
5. migrate canonical schemas transactionally;
6. start Soma in migration/degraded mode if derived rebuild is needed;
7. rebuild projections through durable jobs;
8. run health and saved-query checks;
9. confirm release;
10. clean obsolete projections after rollback window.

## Rollback

Database migrations are either reversible or explicitly marked one-way with restore-from-backup rollback.

The existing Soma self-update mechanism may safely replace the binary, but product migration planning remains an adopter/product responsibility.

## Contract migration records

Every breaking contract change includes:

- prior and new schema;
- migration function/tool;
- stable ID impact;
- reindex requirements;
- donor/migration implications;
- test fixture;
- rollback statement.
