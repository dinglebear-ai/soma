# Soma Context Layer Application Use Cases

All surfaces call these use cases through Soma's existing application layer. No surface owns domain behavior.

## Knowledge sources

### `knowledge.source.create`

Input:

- `SourceRequest`;
- caller principal;
- refresh policy;
- labels and sensitivity.

Output:

- resolved source;
- durable source ID;
- initial refresh job.

### `knowledge.source.list/get/update/delete`

Supports filtered administration, refresh policy, enable/disable, current committed generation, health, storage use, and cleanup impact.

Delete is explicit and reports canonical and derived records affected.

### `knowledge.source.refresh`

Creates a durable job. It never executes source acquisition inside a surface handler.

### `knowledge.document.get`

Returns canonical document metadata and bounded content, subject to authorization.

## Observations

### `observations.source.configure`

Creates or updates receiver/collector configuration for syslog, file tail, Docker, OTLP, heartbeat, inventory, shell history, or AI-session evidence.

### `observations.search`

Performs structured SQL + FTS over canonical records.

### `observations.timeline`

Returns time-ordered records for entities, incidents, runs, services, or hosts.

### `observations.health`

Returns receiver, queue, writer, checkpoint, and projection backlog health.

## Context

### `context.query`

Executes a `ContextQuery` through the context broker and returns a `ContextBundle`.

### `context.entity.resolve`

Resolves aliases and returns candidates with evidence, never silently merging ambiguous entities.

### `context.entity.neighborhood`

Returns bounded graph topology and evidence.

### `context.investigate`

Convenience application use case for temporal entity/incident queries. It composes the same broker, not a separate retrieval stack.

## Graph

### `graph.entity.get/search`

### `graph.relationships.list`

### `graph.path.explain`

### `graph.projection.rebuild`

Rebuild is a durable job and preserves the last ready projection until replacement succeeds.

## Memory

### `memory.remember`

Creates a candidate or active memory according to product policy.

### `memory.recall`

Combines memory-specific ranking and evidence hydration.

### `memory.review/supersede/archive`

All lifecycle changes are explicit and auditable.

## Jobs

### `jobs.list/get/cancel/retry/events`

One job surface covers source refresh, crawl, indexing, observation maintenance, projection, graph rebuild, and memory jobs.

## Administration

### `context.reindex`

Rebuilds selected FTS/vector projections by source, collection, projection version, or embedding profile.

### `context.retention.preview/execute`

Preview is mandatory before destructive execution unless an already-approved automated policy applies.

### `context.doctor`

Checks:

- SQLite integrity/migrations;
- FTS synchronization;
- Qdrant collections and dimensions;
- TEI model and dimensions;
- graph projection state;
- dead citations;
- semantic outbox lag;
- source/receiver health;
- storage budgets.
