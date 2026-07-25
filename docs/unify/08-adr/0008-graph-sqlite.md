# ADR 0008: Use an evidence-first temporal graph backed by SQLite for v1

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Axon and Cortex already use SQLite-based graph/evidence patterns. V1 requires bounded neighborhoods, paths, temporal correlation, and citations, not hyperscale distributed graph traversal.

## Decision

Build soma-graph with an optional SQLite store, deterministic IDs, aliases, evidence, trust, confidence, temporal validity, conflicts, bounded traversals, and rebuildable projections. Product vocabulary remains in Soma.

## Consequences

- No additional database service.
- Graph and canonical observations can join efficiently.
- A dedicated graph database remains possible if measured requirements demand it.

## Rejected alternatives

- Adopt Neo4j or another graph database before benchmarks.
- Store graph only in Qdrant.
- Use model-generated edges without evidence.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
