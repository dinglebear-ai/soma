# ADR 0007: Keep the context broker in Soma's product layer initially

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Cross-store planning depends on Soma authorization, enabled stores, product policies, surface needs, query budgets, and deployment configuration.

## Decision

Shared crates expose stores and retrieval engines. Soma application/runtime modules compose SQL, FTS, Qdrant, graph, memory, reranking, hydration, and synthesis into ContextQuery/ContextBundle use cases.

## Consequences

- No premature public orchestration API.
- Shared engines remain independently useful.
- The broker may be extracted later after multiple consumers prove the seam.

## Rejected alternatives

- Publish a generic pipeline/context broker immediately.
- Put query planning inside the web/API layer.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
