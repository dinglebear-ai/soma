# Product Configuration Contract

Shared crates accept explicit settings. Soma product configuration maps operator settings into those types.

## Suggested hierarchy

```yaml
context:
  storage:
    control_db: /var/lib/soma/control.db
    knowledge_db: /var/lib/soma/knowledge.db
    observations_db: /var/lib/soma/observations.db
    graph_db: /var/lib/soma/graph.db
    memory_db: /var/lib/soma/memory.db
    artifacts_dir: /var/lib/soma/artifacts

  knowledge:
    sources: []
    default_refresh: manual
    maximum_document_bytes: 10485760

  rag:
    embedding:
      provider: tei
      endpoint: http://127.0.0.1:8081
      model: configured-at-image-build
    vector:
      provider: qdrant
      endpoint: http://127.0.0.1:6333
    retrieval:
      dense_limit: 40
      sparse_limit: 40
      fts_limit: 40
      rerank_limit: 30

  observations:
    receivers: []
    collectors: []
    queue_capacity: 20000
    batch_size: 500
    flush_interval_ms: 250
    semantic_projection: selective

  graph:
    projection_version: v1
    max_query_depth: 4
    max_evidence_per_edge: 20

  memory:
    enabled: true
    default_status: candidate

  retention:
    policies: {}

  jobs:
    workers: {}
```

Exact keys are finalized through the existing Soma configuration contract process.

## Rules

- Secrets use Soma's existing secret-reference mechanism and never serialize into public config output.
- Internal service URLs default to loopback.
- Product config versions are explicit and migrated.
- Shared crates do not read environment variables.
- Environment variables, files, CLI flags, and API updates converge through Soma's existing precedence model.
- Config changes that require reindex/restart report that impact before application.
