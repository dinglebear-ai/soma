# Retention, Promotion, and Deletion Contract

## Retention classes

```text
ephemeral
short
standard
extended
permanent
```

Product policy maps classes to time and storage budgets by record kind.

## Canonical versus derived

Canonical record deletion drives cleanup of:

- FTS rows;
- Qdrant points;
- graph evidence and unsupported edges/claims;
- cached summaries;
- artifacts when no other record retains them.

Derived deletion failure becomes durable cleanup debt.

## Promotion

Before short-lived evidence expires, a verified incident, memory, postmortem, or report MAY be promoted into a new canonical record.

Promotion MUST:

- preserve references to source evidence;
- record which evidence may expire;
- carry its own authority/confidence;
- receive an independent retention class;
- never masquerade as the raw evidence.

## Holds

Legal/operator/incident holds override ordinary retention. Holds are explicit, auditable, scoped, and removable.

## Storage budgets

When a budget is reached:

1. enforce per-kind and per-severity policy;
2. preserve held/permanent records;
3. delete oldest eligible canonical records transactionally;
4. create cleanup debt;
5. report deletion counts and any failures.

Silent dropping is prohibited unless the adapter's declared overload policy allows it and emits counters/health warnings.

## Rebuild

FTS, Qdrant, graph, and community summaries can be rebuilt from retained canonical records. Rebuild versions and progress are observable.
