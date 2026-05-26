---
title: "ADR 0006: Generate TypeScript clients from REST OpenAPI"
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

# ADR 0006: Generate TypeScript clients from REST OpenAPI

## Status

Accepted, 2026-05-26.

## Context

Reusable admin and product web apps need reusable TypeScript clients. OpenAPI
generated from action dispatch is useful for docs, but it is too command-shaped
and weakly typed for product web clients.

Rust service repositories already use `serde`, and platform products can derive
OpenAPI schemas from REST DTOs. `openapi-typescript` plus `openapi-fetch` is a
good fit for generated TypeScript clients.

## Decision

Generate TypeScript API clients primarily from REST/admin OpenAPI documents.

Rust REST request and response DTOs should derive `serde` and
`utoipa::ToSchema` where appropriate. Reserve `schemars::JsonSchema` for
standalone JSON Schema consumers such as MCP/action schema projections unless a
DTO has both REST and non-OpenAPI schema consumers.

The preferred generation path is:

```text
Rust REST route DTOs
  -> utoipa::ToSchema + route metadata
  -> product OpenAPI document
  -> openapi-typescript
  -> openapi-fetch or a thin typed wrapper
  -> TypeScript API client package
```

An action-contract manifest may still be generated from `ActionSpec` for MCP
tooling, docs, and optional action-dispatch helpers. It is a separate contract
from REST/OpenAPI and carries its own version.

## Consequences

- A product client is not contract-ready until product REST routes and DTOs
  exist.
- Generated client output must typecheck in CI.
- At least one consumer fixture must typecheck before a client is considered
  reusable.
- Raw typed clients can be wrapped with product-friendly functions when that
  improves ergonomics.

## References

- Source decision ported from Lab ADR
  `docs/adr/0005-typescript-client-generation-from-openapi.md`.
- [`docs/adr/0005-rest-admin-and-mcp-action-surfaces.md`](./0005-rest-admin-and-mcp-action-surfaces.md)
- [`docs/adr/0010-extraction-verification-gates.md`](./0010-extraction-verification-gates.md)

