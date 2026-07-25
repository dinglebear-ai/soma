# Retention and Reindex Operations

## Retention workflow

```text
preview
    ↓
policy decision
    ↓
canonical deletion transaction
    ↓
cleanup debt
    ↓
FTS/vector/graph/artifact cleanup
    ↓
verification
```

Preview reports counts, time ranges, affected citations, holds, promoted records, and estimated reclaimed storage.

## Reindex triggers

- chunker/parser/redaction policy change;
- embedding model/profile change;
- vector payload/index schema change;
- graph projector/version change;
- FTS tokenizer/schema change;
- corruption or dead-citation repair;
- operator request.

## Reindex isolation

Reindex writes a new projection/version while the prior ready projection remains queryable.

For knowledge generations, visibility remains governed by the ledger.

For observations, new semantic points are written idempotently and switched by projection version/filter policy.

## Commands/use cases

```text
context.reindex.preview
context.reindex.execute
context.reindex.status
context.retention.preview
context.retention.execute
context.retention.hold
```

## Validation

After reindex:

- point/row counts reconcile;
- random citations hydrate;
- saved queries meet evaluation thresholds;
- old projection cleanup begins only after acceptance;
- rollback target remains available until the configured window ends.
