# Context Layer Contract Suite

This directory is normative for cross-crate and product integration behavior.

## Contract hierarchy

1. [`schemas.json`](schemas.json) defines transport-neutral record shapes.
2. Markdown contracts define semantics, invariants, state transitions, and authority.
3. Example fixtures define representative serialized forms.
4. Generated references under `docs/generated/context/` are non-normative views.
5. Rust types are authoritative only when they conform to this contract suite.

## Contract generation

Soma's existing `xtask` becomes the contract control plane:

```bash
cargo xtask context-contracts generate
cargo xtask context-contracts check
cargo xtask context-contracts fixtures
cargo xtask context-contracts dependency-graph --check
cargo xtask context-contracts donor-parity --check
```

`generate` MUST:

- gather registered `schemars` definitions from shared and product crates;
- emit one reference-resolved schema bundle;
- generate Markdown field/enum references;
- generate adapter and provider capability matrices;
- record input file hashes;
- generate database and vector-payload references;
- produce a dependency graph and public API inventory.

`check` MUST fail on drift, dangling references, invalid fixtures, stale input hashes, prohibited dependencies, or undocumented breaking changes.

## Compatibility policy

- Adding an optional field is normally additive.
- Adding a required field is breaking.
- Removing or renaming an enum variant is breaking.
- Changing stable-ID inputs is migration-breaking.
- Changing authority, retention, or publication semantics requires an ADR even if serialization remains compatible.
- Projection schemas MAY evolve independently when canonical records remain rebuildable and a reindex path exists.

## Required contracts

- source and generation lifecycle;
- observation lifecycle;
- RAG preparation/index/query;
- graph evidence and temporal semantics;
- context query and bundle;
- citations and authority;
- diagnostics and progress events;
- redaction and bounded metadata;
- retention and deletion;
- database ownership;
- vector payloads and reindexing.
