---
title: "ADR 0009: Execute extraction with isolated lanes and integration ownership"
doc_type: "adr"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-05-26"
---

# ADR 0009: Execute extraction with isolated lanes and integration ownership

## Status

Accepted, 2026-05-26.

## Context

Crate and package extraction touches shared workspace manifests, global
registries, global routers, global app state, serve orchestration, frontend
package manifests, and product-specific runtime files. Parallel work can reduce
elapsed time, but uncontrolled parallel edits move conflict resolution into the
critical path.

## Decision

Execute extraction with one branch/worktree per lane and a dedicated integration
lane for shared wiring.

Product and shared-platform lanes own their local crate/package APIs, tests, and
surface fragments. The integration lane owns shared choke points such as:

- root/workspace `Cargo.toml`
- root package-manager files
- shared library `lib.rs` exports
- application `main.rs`
- global registries
- API router/state composition
- CLI composition
- serve/runtime orchestration
- frontend root/package-manager lockfiles, if introduced
- CI workflow files

Merge completed lanes one at a time. Prefer shared platform lanes first,
smaller product lanes before highly coupled product lanes, frontend lanes after
the first product REST/OpenAPI contract is stable, and standalone binaries last.

## Consequences

- Worktrees avoid live file contention but do not remove merge contention.
- Lane write scopes must remain explicit.
- Product lanes should not wire themselves into the global binary/router unless
  assigned integration ownership for that wave.
- Integration verification is the authoritative signal after merges.

## References

- Source decision ported from Lab ADR
  `docs/adr/0008-extraction-execution-lanes.md`.
- [`docs/adr/0003-shared-platform-and-product-runtime-crates.md`](./0003-shared-platform-and-product-runtime-crates.md)
- [`docs/adr/0010-extraction-verification-gates.md`](./0010-extraction-verification-gates.md)

