---
title: "Cortex Extraction Verification"
created: 2026-08-17
updated: 2026-08-17
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-17"
---

# Cortex Extraction Verification

Extraction verification is layered. Fast crate-local checks catch API and style
errors first; architecture and all-features workspace gates prove the crate does
not destabilize Soma; later Cortex composition waves add product parity smokes.

## Wave 0 proof-crate gates

Run from the Soma extraction worktree:

```bash
cargo fmt --all --check
cargo clippy -p cortex-ingest-core --all-targets --all-features -- -D warnings
cargo test -p cortex-ingest-core --all-features
RUSTDOCFLAGS="-D warnings" cargo doc -p cortex-ingest-core --no-deps --all-features
cargo xtask check-architecture
cargo xtask check-docs
```

The crate test command includes the donor unit tests and
`tests/public_api.rs`, which acts as an external consumer and proves no
Cortex product/runtime dependency is needed.

## Workspace gates

ADR 0010 defines the backend integration truth:

```bash
cargo check --workspace --all-features
cargo nextest run --workspace --all-features
```

If `cargo nextest` is unavailable in a particular developer environment, that
is an environment failure to report and fix, not permission to mark the gate
complete. A temporary local `cargo test --workspace --all-features` run may
provide diagnostic coverage but does not replace the recorded ADR gate.

## Source-parity review

For each donor file moved into a reusable crate:

1. record donor commit and exact source path;
2. compare donor and extracted implementation;
3. classify every semantic diff as required API adaptation, intentional behavior
   change, or defect;
4. port the existing tests with the implementation;
5. add exact output fixtures for persisted/external contracts where valuable;
6. leave no unexplained semantic diff in the review log.

For `cortex-ingest-core`, expected diffs are limited to visibility/rustdoc, the
metadata module/test filename, crate wiring, and dependency-version integration.
The normalization and metadata algorithms themselves should remain unchanged.

## Architecture review

Check both the manifest graph and source semantics:

- package path maps to shared architecture metadata;
- no shared crate imports product crates;
- no domain-like crate exposes storage/transport implementation types;
- optional heavy stacks are not accidentally pulled into minimal profiles;
- public API can be understood without Cortex process globals;
- the crate has one clear responsibility and explicit non-goals.

`cargo xtask check-architecture` is mandatory but not sufficient: it can prove
Cargo dependency direction, not whether a public domain type leaked a database
concept through a generic JSON blob or copied struct.

## Documentation review

Each lane sweeps:

- crate README and rustdoc;
- extraction source inventory and progress tracker;
- Soma architecture/index docs when the set of current crates changes;
- workspace member count or crate lists in contributor instructions;
- examples and commands affected by the new package;
- publication/release docs only when publication status changes.

Run `cargo xtask check-docs` after the sweep.

## Product-parity gates for later waves

As Cortex composition enters Soma, add gates for the exact product surface being
moved. These eventually include:

- Cortex unit/integration suites for service, DB, ingest, inventory, observatory,
  agent, auth, REST, and MCP behavior;
- exact migration/version checks against production-compatible SQLite fixtures;
- REST route and MCP action/schema inventories;
- OAuth/resource/scope tests after the `soma-auth` migration;
- `cortex --help` and command parse/exit smokes;
- HTTP server and stdio MCP startup/shutdown smokes;
- agent start/stop and in-memory/fixture forwarding smokes;
- safe live checks only where they cannot mutate homelab state.

## Review completion

A lane is review-complete only when:

- architecture/API review has no unresolved P0/P1 finding;
- behavior/security review has no unexplained semantic drift;
- all required commands above pass;
- `git diff --check` is clean;
- docs/progress/review logs reflect the actual state rather than intended state.
