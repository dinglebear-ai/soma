---
title: "Architecture Decision Records"
doc_type: "guide"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "template"
source_of_truth: true
last_reviewed: "2026-05-26"
---

# Architecture Decision Records

This directory contains accepted architecture decisions for `rmcp-template` and
the Rust MCP/server family that uses it.

ADRs record decisions that should stay stable across implementation plans,
roadmaps, and temporary migration notes. They complement the topic docs under
`docs/` and should link back to the source material that motivated them.

## Records

- [0001: Stdio-first plugin adapter](./0001-stdio-first-plugin-adapter.md)
- [0002: Extract reusable platform and product packages](./0002-extract-reusable-platform-and-product-packages.md)
- [0003: Split shared platform crates from product runtime crates](./0003-shared-platform-and-product-runtime-crates.md)
- [0004: Compose products through runtime builders](./0004-product-runtime-builders.md)
- [0005: Separate REST admin APIs from MCP action dispatch](./0005-rest-admin-and-mcp-action-surfaces.md)
- [0006: Generate TypeScript clients from REST OpenAPI](./0006-typescript-client-generation-from-openapi.md)
- [0007: Package reusable admin UI as a web boundary](./0007-reusable-web-frontend-package-boundary.md)
- [0008: Use semver with workspace-first extraction and git tags](./0008-versioning-and-distribution.md)
- [0009: Execute extraction with isolated lanes and integration ownership](./0009-extraction-execution-lanes.md)
- [0010: Require boundary and generated-client verification](./0010-extraction-verification-gates.md)

