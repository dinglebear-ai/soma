---
title: "Architecture Decision Records"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: true
last_reviewed: "2026-08-17"
---

# Architecture Decision Records

This directory contains accepted architecture decisions for `soma` and
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
- [0011: Keep Soma product-first and template-second](./0011-product-first-template-second.md)
- [0012: rmcp-traces targets RMCP 2.2.0](./0012-rmcp-traces-rmcp-2-2.md)
- [0013: Keep Python provider authoring embedded and contract-first](./0013-python-provider-authoring-boundary.md)
- [0014: Extract Cortex as reusable shared crates](./0014-extract-cortex-as-reusable-shared-crates.md)
