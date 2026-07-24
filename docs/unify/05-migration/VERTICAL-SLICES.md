# Vertical Slice Plan

## Slice 0: Gateway and web shell complete

Precondition owned by current Soma work. Context pages plug into the finished shell.

## Slice 1: Contract landing zone

Deliver:

- donor baselines;
- crate catalog;
- shared schema bundle;
- `xtask` generation/check commands;
- dependency gates;
- fixture registry.

## Slice 2: Shared foundations

Deliver:

- `soma-primitives`;
- `soma-sanitize`;
- `soma-process`;
- standalone consumer tests.

## Slice 3: Local knowledge

Deliver local route, adapter, ledger, RAG, FTS/Qdrant, and all existing surface projections.

**Demo:** index Soma's repository and answer a code/docs query with exact citations.

## Slice 4: Durable jobs

Wrap refresh/index/reindex with durable jobs, progress, cancellation, and restart recovery.

## Slice 5: AI sessions

Unify Claude, Codex, and Gemini parsing. Emit knowledge documents, chunks, tool/MCP observations, and graph candidates.

## Slice 6: Web crawling

Add Spider/Chrome crawl engine and page/site/docs source adapter.

## Slice 7: Remaining knowledge adapters

GitHub, feeds, YouTube, Reddit, crates.io, npm, PyPI, uploads, and tool outputs.

## Slice 8: Observation foundation

Canonical observation SQLite/FTS, ingest runtime, health, retention, file tail, and syslog.

## Slice 9: Remaining observation adapters

Docker, OTLP, heartbeat, inventory/configuration, shell history, sessions.

## Slice 10: Semantic observation projection

Outbox, grouping/correlation policies, shared RAG indexing, cleanup, and metrics.

## Slice 11: Evidence graph

Unify Axon and Cortex graph behavior, implement deterministic projectors, entity resolution, temporal evidence, neighborhoods, and paths.

## Slice 12: Context broker and GraphRAG

Basic hybrid, entity-local, and temporal investigation modes across all stores.

## Slice 13: Memory

Evidence-backed memory lifecycle and context integration.

## Slice 14: Hardening and cutover

Migration, backups, reindex, performance, security, north-star evaluation, and donor retirement.

## WIP limit

At most:

- one active vertical product slice;
- one shared foundation/publication task.

Parallel leaf adapters begin only after their common contract is accepted.
