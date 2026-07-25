# Context Layer Observability

Soma's existing observability and tracing crates remain authoritative.

## Trace spans

Required high-level spans:

```text
source.route
source.discover
source.diff
source.acquire
document.prepare
embedding.batch
vector.upsert
generation.publish
observation.receive
observation.persist
projection.semantic
graph.project
context.plan
context.retrieve.sql
context.retrieve.fts
context.retrieve.vector
context.retrieve.graph
context.rerank
context.hydrate
context.synthesize
memory.recall
```

## Correlation

Spans/events carry applicable:

```text
request_id
job_id
source_id
generation_id
document_id
observation_id
projection_task_id
entity_id
context_query_id
```

No secret or unbounded content enters span attributes.

## Metrics

- source refresh duration/success;
- documents/chunks/bytes;
- embedding batches/tokens/latency;
- vector upsert/search;
- receiver records/drops;
- writer queue/batch/latency;
- checkpoint/outbox lag;
- retention deletes;
- graph candidates/merges/conflicts;
- context lane latency/results;
- citation hydration failures;
- synthesis claims/citation coverage;
- storage size;
- job states.

## Logs

Operational logs are structured and bounded. Errors use stable codes. High-cardinality IDs are trace/log fields, not metric labels unless carefully bounded.

## Self-observation

Soma MAY ingest selected own logs/traces as ordinary observations, but recursion is bounded and source-tagged to avoid endlessly indexing its own indexing messages.
