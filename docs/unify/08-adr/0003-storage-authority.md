# ADR 0003: SQLite and durable artifacts are canonical; Qdrant and graph summaries are derived

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Operational records need exact filtering, time, retention, and durable citations. Vector indexes are valuable for meaning but cannot be transactionally authoritative with SQLite.

## Decision

Canonical source, document, observation, configuration, session, job, graph-evidence, and memory records live in SQLite or durable artifacts. FTS, Qdrant points, graph/community summaries, and generated synthesis are rebuildable projections. Graph records may be durable projections but every edge resolves to canonical evidence.

## Consequences

- TEI/Qdrant outages do not lose canonical writes.
- Re-embedding and reindexing are safe.
- Citations resolve to exact records.
- Backup prioritizes canonical data.

## Rejected alternatives

- Put every raw record only in Qdrant.
- Store no semantic projection.
- Use Qdrant payloads as canonical source text.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
