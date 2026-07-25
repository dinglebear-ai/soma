# ADR 0002: Use multiple ingestion protocols with one context plane

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Axon refreshes finite sources using manifests and committed generations. Cortex receives continuing and periodic operational observations using queues, checkpoints, retention, and append semantics.

## Decision

Keep source-generation and observation-stream lifecycles separate. Converge through CanonicalRef, Citation, IndexDocument, EvidenceRef, GraphCandidate, and MemoryCandidate.

## Consequences

- Each donor's correctness model remains intact.
- The context broker unifies query, not raw acquisition.
- Shared projectors create cross-domain semantic and graph records.

## Rejected alternatives

- Force logs through source manifests/generations.
- Treat repositories as append-only event streams.
- Leave systems completely disconnected.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
