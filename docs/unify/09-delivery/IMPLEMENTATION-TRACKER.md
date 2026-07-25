# Implementation Tracker

The machine-readable source is [`../05-migration/capability-matrix.yaml`](../05-migration/capability-matrix.yaml).

Generated views SHOULD show:

- capability status;
- crate status;
- active PRs;
- donor paths covered;
- parity fixtures;
- surface completion;
- operations completion;
- risks and open decisions;
- package readiness.

## Allowed statuses

Capability:

```text
not_started
characterizing
contracted
implementing
composed
parity_verifying
product_verifying
complete
blocked
```

Crate:

```text
candidate
boundary_approved
implemented
soma_consumed
external_consumer_verified
api_reviewed
publish_ready
published
blocked
```

No generic `in_progress`.

## Progress measurement

Primary:

- completed capabilities;
- required E2E scenarios;
- donor capabilities retired;
- canonical data migrated;
- north-star evidence coverage.

Secondary:

- crates implemented/published;
- donor paths mapped;
- contract fixtures.

Lines moved are not progress.
