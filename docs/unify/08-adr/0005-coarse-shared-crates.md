# ADR 0005: Extract coarse reusable crates instead of mirroring Axon's 23 crates

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Axon's recent refactor created useful internal boundaries, but some crates are cycle breakers or product façades rather than ideal public packages.

## Decision

Create 16 coarse shared crates organized by reusable capability. Every package uses `soma-<one-word>`. Dissolve Axon API/core/service/surface packages into domain-owned types, existing Soma foundations, or product composition.

## Consequences

- Fewer public packages and release trains.
- Short, single-concept package names with no compound suffixes.
- Short, single-concept package names with no compound suffixes.
- RAG remains one coherent package with internal modules/features.
- Product policy stays outside shared crates.

## Rejected alternatives

- Copy all 23 Axon crates into Soma.
- Create one giant context crate.
- Split every parser/adapter into a package immediately.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
