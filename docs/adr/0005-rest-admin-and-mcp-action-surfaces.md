---
title: "ADR 0005: Separate REST admin APIs from MCP action dispatch"
doc_type: "adr"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-06-17"
---

# ADR 0005: Separate REST admin APIs from MCP action dispatch

## Status

Accepted, 2026-05-26.

## Context

The server family benefits from compact MCP exposure: one tool per service with
an `action` plus parameters shape. That keeps agent tool lists small. Web/admin
apps and generated clients need a different shape: resource-oriented HTTP
routes with typed request and response DTOs.

Using one action-dispatch API shape for every surface would make web clients
less conventional. Duplicating business logic per surface would create drift.

## Decision

Support two external API shapes over shared product runtime/domain logic:

```text
product runtime/domain logic
  -> REST/admin handlers
  -> MCP action handlers
  -> CLI handlers
```

REST/admin HTTP is the primary surface for web apps and generated TypeScript
clients. It should use direct resource- or action-specific routes, typed request
and response DTOs, and product OpenAPI documents. The template does not expose
a REST action envelope.

MCP action dispatch remains the primary compact agent/tool surface.
`crates/rtemplate-service/src/actions.rs` remains the source of truth for MCP
discovery, action help, action schemas, and destructive-action metadata.

CLI remains an operator adapter over the same runtime/domain logic or shared
dispatch layer. Destructive CLI operations must respect the same destructive
metadata as MCP.

## Consequences

- REST routes and MCP actions must call the same product runtime/domain
  functions.
- ActionSpec-derived OpenAPI is transitional and not the final typed web client
  contract for platform products.
- REST error responses use canonical product error envelopes or documented
  product extensions.
- MCP-only behavior must be documented as a protocol-specific exception.

## References

- Source decision ported from Lab ADR
  `docs/adr/0004-rest-admin-and-mcp-action-surfaces.md`.
- [`docs/adr/0001-stdio-first-plugin-adapter.md`](./0001-stdio-first-plugin-adapter.md)
- [`docs/adr/0006-typescript-client-generation-from-openapi.md`](./0006-typescript-client-generation-from-openapi.md)
