---
title: "ADR 0008: Use semver with workspace-first extraction and git tags"
doc_type: "adr"
status: "active"
owner: "soma"
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

Release-please owns release PRs, changelog entries, version bumps, `v*` tags,
and GitHub Releases. `release/components.toml` remains the local inventory for
the single shipped `soma` component and every version-bearing file that must be
kept in sync after release-please updates the canonical release manifest.

Use `cargo xtask check-version-sync` in PR CI to verify version-bearing files
agree. On release-please PR branches, run
`cargo xtask sync-release-please-version` to copy the version from
`.release-please-manifest.json` into the derived files declared by
`release/components.toml`.

## Consequences

- Boundary stability is proven before repository splits or public publishing.
- Breaking changes to REST routes, response shapes, auth requirements, MCP
  action params, package exports, or dependency direction require a major
  version bump or compatibility alias.
- Runtime shipping-path changes should use Conventional Commits so
  release-please can choose the next `soma` version and release notes before
  merge.
- The externalization decision is deferred until in-repo boundaries pass tests
  and have consumer fixtures.

## References

- Source decision ported from Lab ADR
  `docs/adr/0007-versioning-and-distribution.md`.
- [`docs/adr/0002-extract-reusable-platform-and-product-packages.md`](./0002-extract-reusable-platform-and-product-packages.md)
- [`docs/adr/0010-extraction-verification-gates.md`](./0010-extraction-verification-gates.md)
