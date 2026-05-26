---
title: "ADR 0002: Extract reusable platform and product packages"
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

# ADR 0002: Extract reusable platform and product packages

## Status

Accepted, 2026-05-26.

## Context

The Rust MCP/server family has grown beyond single binaries. Gateway, auth,
configuration, runtime composition, marketplace/plugin handling, setup, doctor,
logs, web shells, and product-specific action surfaces are useful as reusable
capabilities.

Future products should consume these capabilities through package dependencies
instead of copying source from a full application or depending on one product
binary.

## Decision

Extract reusable capabilities into Rust crates, TypeScript packages, and thin
standalone binaries while preserving each full product binary as a composition
of those same boundaries.

Extraction starts inside the current repository or workspace. Moving packages to
separate repositories or publishing them is deferred until the boundaries have
stable APIs, tests, and at least one consumer fixture where appropriate.

## Consequences

- Full product binaries remain available during migration.
- New products depend on package APIs instead of vendored source.
- Extraction work must preserve accumulated behavior rather than rewrite
  product runtimes from scratch.
- Package boundaries become architectural contracts, not just folder names.

## References

- Source decision ported from Lab ADR
  `docs/adr/0001-extract-lab-as-reusable-packages.md`.
- [`docs/adr/0003-shared-platform-and-product-runtime-crates.md`](./0003-shared-platform-and-product-runtime-crates.md)
- [`docs/adr/0004-product-runtime-builders.md`](./0004-product-runtime-builders.md)

