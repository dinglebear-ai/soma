---
title: "ADR 0003: Split shared platform crates from product runtime crates"
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

# ADR 0003: Split shared platform crates from product runtime crates

## Status

Accepted, 2026-05-26.

## Context

Reusable infrastructure concerns and reusable product capabilities are often
mixed inside a product binary. If shared infrastructure imports product code,
future products inherit unnecessary dependencies and extracted platform crates
cannot stay small.

## Decision

Use two backend crate classes.

Shared platform crates provide reusable infrastructure. Examples include auth,
config, runtime composition, catalog/surface definitions, observability, setup,
and common transport utilities.

Product runtime crates own reusable product capabilities. Examples include
gateway, marketplace, ACP, fleet/nodes, stash, OAuth flows, logs, workspace,
doctor, and service-specific runtime modules.

Allowed backend dependency direction is:

```text
application binary
  -> product runtime crates
  -> shared platform crates
  -> external crates
```

Shared crates must not depend on product runtime crates. Product crates must not
depend on sibling product crates unless an explicit exception is added to that
product's extraction contract.

## Consequences

- Cross-product orchestration belongs in application composition layers or small
  shared interfaces.
- Shared crates must expose narrow public APIs and avoid broad internal
  re-exports.
- Product crates can evolve independently without dragging sibling runtime code
  into consumers.

## References

- Source decision ported from Lab ADR
  `docs/adr/0002-shared-platform-and-product-runtime-crates.md`.
- [`docs/adr/0002-extract-reusable-platform-and-product-packages.md`](./0002-extract-reusable-platform-and-product-packages.md)
- [`docs/adr/0009-extraction-execution-lanes.md`](./0009-extraction-execution-lanes.md)

