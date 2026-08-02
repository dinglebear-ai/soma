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
| product-family | 6 | `feat/operations-surface-contracts` | `feat/operations-schema-diagnostics` | `~/workspace/soma/.worktrees/operations-surface-contracts` | O2 closed parameter schemas and diagnostic surface projections | #265 | parity_verifying |
| product-family | 7 | `feat/operations-result-schemas` | `feat/operations-surface-contracts` | `~/workspace/soma/.worktrees/operations-result-schemas` | O2 closed canonical result payload schemas | #266 | parity_verifying |
| product-family | 8 | `feat/synapse-product-import` | `feat/operations-result-schemas` | `~/workspace/soma/.worktrees/synapse-product-import` | Locked standalone Synapse donor snapshot with recorded provenance | #268 | parity_verifying |
| product-family | 9 | `feat/synapse-compat-adapters` | `feat/synapse-product-import` | `~/workspace/soma/.worktrees/synapse-compat-adapters` | Native catalog, Flux/Scout normalization, result and diagnostic compatibility adapters | #269 | parity_verifying |
| product-family | 10 | `feat/fleet-foundation` | `feat/synapse-compat-adapters` | `~/workspace/soma/.worktrees/fleet-foundation` | Neutral topology, pooling, OpenSSH, forwarding, transfer lifecycle, and bounded fanout | #270 | parity_verifying |
| product-family | 11 | `feat/infra-foundation` | `feat/fleet-foundation` | `~/workspace/soma/.worktrees/infra-foundation` | Read-only host, Docker, Compose, and confined filesystem engines | #271 | parity_verifying |

Every additional row in a stack must use the branch immediately above it as its PR base until the lower PR merges and the stack is restacked.

Current operations-foundation evidence: extraction spec, code map, domain models, schema contract, Draft 2020-12 JSON Schema, twelve-PR implementation plan, current Synapse donor lock, 49 unit tests, strict Clippy, warning-free rustdoc, external-consumer compile, architecture check, and xtask tests. The next slice now locks all donor-provided legacy semantics for 59 operations with exact scopes, dispatch shapes, parameters, transport/destructive metadata, source provenance, and a deterministic digest. The canonical-classification slice now locks target kind, access, risk, reversibility, planning, progress, cancellation, verification, fanout, retry, idempotency, evidence, requirements, and parameter groups for all 59 operations. The schema-diagnostics slice now binds each operation/version to deterministic parameter and result `SchemaId` values and a validated machine-stable diagnostic vocabulary. The surface-contract slice now locks 59 closed canonical parameter schemas and the complete 33-code projections for CLI exit, HTTP status, MCP error data, event severity, retry, and terminal behavior. The result-schema slice now binds all 59 operations to closed canonical payload schemas across 13 normalized output families. The product-import slice records the Synapse donor provenance and reviewed import-tree object for all 386 tracked files under the temporary `crates/synapse/import` boundary, with donor tests and deterministic snapshot verification. The compatibility-adapter slice adds the native `synapse-application` workspace crate. It embeds and cross-validates all five canonical artifacts, owns all 59 legacy bindings, normalizes Flux/Scout requests through closed parameter schemas, validates and projects canonical results, derives closed legacy MCP schemas, and enforces all 33 diagnostic mappings per operation without linking the imported donor workspace. The fleet-foundation slice adds the native `soma-fleet` shared crate with revision-bound topology and pooling, bounded local process execution, strict OpenSSH plans and execution, owner-only forwarding, observable transfer lifecycle, cancellation-aware fanout, and explicit post-spawn remote uncertainty. The infra-foundation slice adds the native `soma-infra` shared crate with typed host inspection, neutral Docker read traits and a revision-bound local Bollard adapter, shell-free Compose listing/status/config, and Linux descriptor-confined filesystem stat/read/hash. The infra-read-expansion slice adds Docker disk usage, bounded one-shot container logs and stats, Compose logs, typed process snapshots, validated syslog/journal/dmesg/auth reads, and structured ZFS pool/dataset/snapshot tables. The locked import snapshot remains unchanged; filesystem list/tail, remote Docker composition, differential parity fixtures, and Synapse delegation remain the next gates.

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
