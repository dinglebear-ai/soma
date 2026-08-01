---
title: "ADR 0012: Build a multi-distribution monorepo"
created: 2026-07-31
updated: 2026-07-31
---

# ADR 0012: Build a multi-distribution monorepo

**Status:** Accepted
**Date:** 2026-07-31

## Context

Soma is absorbing reusable implementation from Labby, Axon, Cortex, and Synapse. Folding those engines into one source repository must not make Soma the only deployable product or reduce the donor products to compatibility wrappers.

The product family has five distinct deployment goals:

- Labby is a complete standalone MCP gateway;
- Axon is a complete standalone research and RAG engine;
- Cortex is a complete standalone observability-ingestion and evidence-graph engine;
- Synapse is a complete standalone infrastructure operations engine;
- Soma is the integrated superset product.

Maintaining five manually synchronized repositories would duplicate engines, drift contracts, and make fixes expensive to propagate. Making every capability private to the Soma binary would remove focused deployments and force users to install the entire platform.

## Decision

Soma becomes a multi-distribution monorepo.

- Product-neutral mechanisms live under `crates/shared/*`.
- Product-specific policy and compatibility behavior live under `crates/<product>/*`.
- Each final executable and runtime composition root lives under `apps/<product>`.
- Labby, Axon, Cortex, Synapse, and Soma each retain independent configuration, storage, surfaces, tests, packaging, and releases.
- Soma may consume neutral engines in-process or connect to a separately deployed product through a stable remote contract.
- Shared crates may have a primary product steward, but they must remain usable without any product crate or binary.

The target composition roots are:

```text
apps/labby
apps/axon
apps/cortex
apps/synapse
apps/soma
```

The dependency direction is:

```text
shared crates
    ^
    |
product application and surface crates
    ^
    |
apps/<product> composition root
```

A product crate may depend on shared crates. A shared crate must not depend on a product crate or application.

## Standalone product contract

A distribution is considered standalone only when it satisfies all of the following:

1. Its primary capability works without Soma or another product process.
2. It owns a complete configuration and runtime lifecycle.
3. It owns its default storage namespace and migrations where stateful.
4. It exposes sufficient CLI, API, MCP, and web surfaces for its domain, with documented exceptions.
5. It has independent health, readiness, doctor, backup, restore, upgrade, and packaging behavior where applicable.
6. It has standalone unit, integration, end-to-end, migration, and release tests.
7. It can be versioned and released independently from the other distributions.

## Consequences

- One implementation can serve focused products and the integrated platform.
- Product boundaries become compiler-enforced instead of naming conventions.
- Soma integrations must use neutral engine contracts or remote clients, not another product's CLI, MCP, HTTP, or web internals.
- Each extraction requires both an embedded-consumer test and a standalone-product test.
- Release automation must support coordinated and independent product releases.
- The repository layout and architecture checks must expand beyond a single `apps/soma` composition root.

## Rejected alternatives

- Keep all products in separate repositories and manually synchronize shared code.
- Make Soma the only distribution and retain donor names as feature flags.
- Make standalone products thin wrappers around the Soma application crate.
- Let shared crates read product environment variables or import product policy.

## Revisit when

Revisit only if measured release, build, or security constraints prove that one product requires a separate source repository. A split must preserve shared contracts and cannot reintroduce manual code duplication.
