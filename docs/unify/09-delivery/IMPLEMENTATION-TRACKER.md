---
title: "Implementation Tracker"
created: 2026-07-24
updated: 2026-08-01
---

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

## Active stacked PR trains

| Stack | Position | Branch | Base | Isolated worktree | Capability | PR | Status |
|---|---:|---|---|---|---|---|---|
| product-family | 1 | `feat/product-family-architecture` | `main` | `~/workspace/soma/.worktrees/product-family-architecture` | O0 contracts and architecture | #257 | contracted |
| product-family | 2 | `feat/operations-foundation` | `feat/product-family-architecture` | `~/workspace/soma/.worktrees/operations-foundation` | O2 operations contracts, models, schema, and plan | #260 | external_consumer_verified |
| product-family | 3 | `feat/operations-semantic-parity` | `feat/operations-foundation` | `~/workspace/soma/.worktrees/operations-semantic-parity` | O2 donor legacy semantic fixture and parity test | #261 | parity_verifying |
| product-family | 4 | `feat/operations-canonical-classification` | `feat/operations-semantic-parity` | `~/workspace/soma/.worktrees/operations-canonical-classification` | O2 canonical target, safety, lifecycle, and capability classifications | #262 | parity_verifying |
| product-family | 5 | `feat/operations-schema-diagnostics` | `feat/operations-canonical-classification` | `~/workspace/soma/.worktrees/operations-schema-diagnostics` | O2 versioned schema identities and stable diagnostic vocabulary | #264 | parity_verifying |

Every additional row in a stack must use the branch immediately above it as its PR base until the lower PR merges and the stack is restacked.

Current operations-foundation evidence: extraction spec, code map, domain models, schema contract, Draft 2020-12 JSON Schema, twelve-PR implementation plan, current Synapse donor lock, 49 unit tests, strict Clippy, warning-free rustdoc, external-consumer compile, architecture check, and xtask tests. The next slice now locks all donor-provided legacy semantics for 59 operations with exact scopes, dispatch shapes, parameters, transport/destructive metadata, source provenance, and a deterministic digest. The canonical-classification slice now locks target kind, access, risk, reversibility, planning, progress, cancellation, verification, fanout, retry, idempotency, evidence, requirements, and parameter groups for all 59 operations. The schema-diagnostics slice now binds each operation/version to deterministic parameter and result `SchemaId` values and a validated machine-stable diagnostic vocabulary. Concrete field-level request/result schemas and CLI/REST/MCP diagnostic projections remain the next gate.

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
