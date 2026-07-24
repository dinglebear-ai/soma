# ADR 0011: Use a transactional semantic outbox

**Status:** Proposed  
**Date:** 2026-07-21

## Context

SQLite canonical writes and Qdrant/TEI operations cannot share one atomic transaction. Blocking observation ingestion on semantic services would reduce reliability.

## Decision

Canonical SQLite transactions enqueue deterministic semantic projection tasks. Background workers claim, retry, dead-letter, and idempotently upsert projections. Canonical acknowledgement does not wait for TEI/Qdrant.

## Consequences

- No lost projection intent.
- Provider outages do not lose logs.
- Reindex and policy changes reuse the same mechanism.

## Rejected alternatives

- Best-effort fire-and-forget vector writes.
- Synchronous dual writes.
- Make Qdrant authoritative.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
