# ADR 0004: Vectorize selected semantic units, not every observation row

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Embedding every heartbeat, repeated info log, DNS event, and metric sample would inflate cost and reduce retrieval quality.

## Decision

All observations remain structurally searchable in SQLite/FTS. Semantic policy projects high-value events, incident windows, state changes, sessions, tool/MCP activity, errors, and anomalies into IndexDocument records through a durable outbox.

## Consequences

- Vector volume tracks information value.
- Canonical ingestion stays fast and available.
- Policies can evolve and reproject.

## Rejected alternatives

- Embed every row.
- Never embed operational data.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
