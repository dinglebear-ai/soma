---
title: Validation Report
created: 2026-07-24
updated: 2026-07-31
---

# Validation Report

**Generated:** 2026-07-31
**Result:** PASS

## Package checks

| Check | Result |
|---|---|
| Package files | 119 |
| Manifest content files | 117 |
| Markdown documents | 104 |
| Approximate documentation words | 36,764 |
| Planned shared crates | 19 |
| Detailed context crate specifications | 16 |
| Operations boundary specifications | 3 planned in catalog and ADRs; implementation specs land with extraction PRs |
| `soma-<one-word>` naming | PASS |
| ADR count | 13 |
| Machine-readable capability entries | 13 |
| JSON/YAML/TOML parse | PASS |
| Draft 2020-12 context schema validity | PASS |
| Context schema fixtures | 4 PASS |
| Synapse operation compatibility fixture | 59 operations PASS |
| Local Markdown links checked | 42 |
| Broken local Markdown links | 0 |
| Forbidden APM/Incus mission schema keys | 0 |
| Integrity checksum entries | 118 |

## Executable checks

| Check | Result |
|---|---|
| Targeted architecture tests | 25 PASS |
| Complete xtask tests | 206 PASS |
| Workspace architecture graph | PASS: 36 packages, 85 internal edges |
| xtask clippy with warnings denied | PASS |
| Rust formatting | PASS |
| Python syntax | PASS |
| Git diff whitespace | PASS |
| Documentation manifest freshness | PASS |
| Synapse donor fixture parity | PASS against pinned `origin/main` |

## Product-family decisions

- Labby, Axon, Cortex, Synapse, and Soma are complete distributions with independent composition roots.
- Shared crates remain independently consumable and may not import any product crate.
- Product crates may not import another product's internals; integration occurs through neutral shared engines or stable remote contracts.
- Synapse is the operations-plane product and primary steward of `soma-ops`, `soma-fleet`, and `soma-infra`.
- The existing Incus REST client remains neutral under `crates/shared`.
- Cortex consumes neutral operation lifecycle events and never gains execution authority.
- Axon and Cortex retain separate ingestion lifecycles while remaining complete standalone products.

## Donor locks

| Donor | Full pinned commit | Validation in this slice |
|---|---|---|
| Soma | `00a3336dab84a1ae847fc814d3af917f46c90b47` | Destination workspace and architecture graph validated |
| Axon | `346238ac31a89f0fd4bddca36f0628a11b8edd98` | Full donor ref pinned |
| Cortex | `3d75d109cc3531d1b18c9d32c4059566651cd863` | Full donor ref pinned |
| Synapse | `b92552900c1458aa03b370c80edc812884c77f31` | Operation source hashed and all 59 operations parsed into parity fixture |

The canonical donor lock is [`05-migration/donors.lock.toml`](05-migration/donors.lock.toml).

## Compatibility fixtures

The original context fixtures remain valid: `source-request.json`, `observation-record.json`, `graph-candidate.json`, and `context-query.json`.

The new [`03-contracts/examples/synapse-operations.json`](03-contracts/examples/synapse-operations.json) fixture records the full donor commit and source SHA-256, all 59 legacy operation identities, canonical neutral names, Flux/Scout ownership, scope and destructive classifications, transport availability, required parameter groups, and donor source lines.

## Shared-crate coverage

The original 16 context crates retain detailed per-crate specifications. The operations extension adds three accepted coarse boundaries: `soma-ops`, `soma-fleet`, and `soma-infra`.

Their implementation-level public APIs and independent-consumer fixtures are intentionally deferred to the stacked extraction PRs, where compiler feedback and real Synapse consumers can stabilize the boundaries before publication.

## Integrity

`CHECKSUMS.sha256` covers every package file except itself. `MANIFEST.yaml` records package metadata, full donor baselines, scope, entry points, counts, and per-file hashes for all 117 non-self-referential content files.

Both artifacts are generated and checked by `scripts/generate-unify-manifest.py`. Check mode was verified not to modify either artifact.

## Remaining verification delegated to later PRs

This architecture slice does not claim runtime parity for code that has not yet been imported or extracted. Later stacked PRs must still:

- import Synapse history and build its standalone binary unchanged;
- run Synapse's complete Rust, web, packaging, and security suites;
- compile each extracted shared crate with an unrelated external consumer fixture;
- prove standalone and embedded operation behavior independently;
- prove Soma embedded operations and Soma-to-Labby-to-Synapse remote operations against the same contracts;
- compile and validate the Axon and Cortex standalone distributions as their product roots land;
- verify crates.io package-name availability before publishing new shared crates;
- preserve migration and rollback evidence before donor-repository cutover.
