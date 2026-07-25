# Data Flow

## Refreshable knowledge

```text
SourceRequest
  -> route
  -> sources.discover
  -> SourceManifest
  -> ledger.diff
  -> sources.acquire
  -> SourceDocument[]
  -> rag.prepare
  -> PreparedChunk[]
  -> rag.embed
  -> lexical + vector staging
  -> ledger.publish
  -> graph projection
```

## Continuing observations

```text
protocol/collector
  -> collectors
  -> ObservationRecord
  -> ingest queue
  -> SQLite canonical write + FTS
  -> semantic outbox in same transaction
  -> evidence graph projector
  -> selective RAG projection worker
  -> Qdrant
```

## Query

```text
ContextQuery
  -> classify intent and resolve filters
  -> SQL/FTS lane
  -> dense/sparse vector lane
  -> graph seed and traversal lane
  -> memory lane
  -> hydrate canonical evidence
  -> deduplicate and rerank
  -> context budget
  -> optional synthesis
  -> ContextBundle with citations
```

## Failure behavior

- Source acquisition failures MUST NOT publish an incomplete generation.
- Observation projection failures MUST NOT block canonical observation writes.
- Qdrant outages MUST create retryable projection debt.
- Graph rebuild failures MUST leave the previous committed projection available.
- Synthesis failures MUST still return retrieved evidence when policy permits.
