---
title: "ADR 0004: Compose products through runtime builders"
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

# ADR 0004: Compose products through runtime builders

## Status

Accepted, 2026-05-26.

## Context

Full product binaries wire registries, routers, global state, service managers,
OAuth, logs, MCP, HTTP, CLI, and web assets in startup code. That makes
standalone product binaries and external product reuse difficult.

## Decision

Every product runtime crate must expose a library-level runtime builder or
equivalent composition API. Builders accept configuration and dependencies
explicitly, then return the product surface fragments needed by application
binaries.

The exact types may vary by product, but the contract is:

```rust
pub struct ProductRuntime {
    pub router: Option<axum::Router>,
    pub registry: Option<ToolRegistry>,
    pub catalog: Option<Catalog>,
}

pub struct ProductRuntimeBuilder {
    // explicit dependencies only
}

impl ProductRuntimeBuilder {
    pub async fn build(self) -> anyhow::Result<ProductRuntime>;
}
```

Standalone binaries must be thin wrappers over these library APIs and must not
own product business logic.

## Consequences

- Future products can compose only the product runtimes they need.
- Full product binaries become composition binaries instead of the only owners
  of product wiring.
- Global runtime handles are compatibility shims to remove or isolate during
  extraction.
- Product builder tests become the proof that a product can run outside the
  full binary.

## References

- Source decision ported from Lab ADR
  `docs/adr/0003-product-runtime-builders.md`.
- [`docs/adr/0003-shared-platform-and-product-runtime-crates.md`](./0003-shared-platform-and-product-runtime-crates.md)

