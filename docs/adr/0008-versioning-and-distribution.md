---
title: "ADR 0008: Use semver with workspace-first extraction and git tags"
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

# ADR 0008: Use semver with workspace-first extraction and git tags

## Status

Accepted, 2026-05-26.

## Context

Extraction needs package boundaries before it needs public package
distribution. Publishing too early would freeze unstable APIs, but future
products still need versioned dependencies once a boundary is reusable.

npm has an important caveat: direct git dependencies must resolve to a real
package root. npm does not install subpackages buried inside a git workspace as
independent packages.

## Decision

Start extraction as internal workspace crates and packages. Use semver for all
extracted Rust crates and TypeScript packages.

During active extraction, path dependencies are allowed inside development
workspaces. External consumers should eventually use versioned git tags or
published packages.

Publishing to `crates.io` or npm is optional and not required for first reuse.
If frontend packages are consumed through git dependencies, each dependency must
resolve to a repository/package root with its own `package.json`, or the project
must use a workspace-aware release flow that publishes/packages each dependency
explicitly.

REST APIs stay under explicit versions such as `/v1`. OpenAPI documents and MCP
action-contract manifests carry versions because they are separate surfaces.

`release/components.toml` is the versioning source of truth for this repository.
It declares the single shipped `template` component, the `v` tag prefix, the
release workflow, shipping paths that require a version bump, and every
version-bearing file. The manifest also records files that must stay
versionless, such as Claude/Codex/Gemini plugin manifests whose marketplace
version is derived from the git commit SHA.

Use `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
in PR CI. The PR mode compares from merge-base to avoid false failures from
changes that exist only on the base branch. Use `cargo xtask release-plan --head
HEAD --mode main --json` on `main` to plan auto-tags from the latest matching
semver tag.

## Consequences

- Boundary stability is proven before repository splits or public publishing.
- Breaking changes to REST routes, response shapes, auth requirements, MCP
  action params, package exports, or dependency direction require a major
  version bump or compatibility alias.
- Runtime shipping-path changes require the `template` component version to be
  greater than the latest `v*` semver tag before merge.
- The externalization decision is deferred until in-repo boundaries pass tests
  and have consumer fixtures.

## References

- Source decision ported from Lab ADR
  `docs/adr/0007-versioning-and-distribution.md`.
- [`docs/adr/0002-extract-reusable-platform-and-product-packages.md`](./0002-extract-reusable-platform-and-product-packages.md)
- [`docs/adr/0010-extraction-verification-gates.md`](./0010-extraction-verification-gates.md)
