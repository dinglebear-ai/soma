# Storage Architecture

## Logical stores

| Store | Canonical responsibilities |
|---|---|
| `control.db` | product configuration, provider state, source registration, job metadata where existing ownership permits |
| `knowledge.db` | source generations, manifests, documents, chunk metadata, knowledge FTS |
| `observations.db` | logs, telemetry, inventory, config snapshots, shell/session events, observation FTS, semantic outbox |
| `graph.db` | entities, aliases, relationships, claims, evidence, projection state, community metadata |
| `memory.db` | memories, review, reinforcement, contradiction, supersession |
| Artifact filesystem | raw captures, source snapshots, screenshots, normalized artifacts |
| Qdrant | semantic projections only |

Physical databases MAY be consolidated initially, but table ownership and rebuild semantics MUST remain explicit.

## Qdrant collections

```text
soma_knowledge_v1
soma_observations_v1
soma_memory_v1
soma_graph_v1
```

Each point MUST contain:

- deterministic point ID;
- canonical references;
- projection kind and version;
- content hash;
- embedding model identity;
- bounded filterable metadata;
- safe excerpt only.

## FTS5

Knowledge and observation FTS MUST remain available even when Qdrant is healthy. FTS is authoritative for exact lexical matching, not for canonical content.

## Transaction boundary

A canonical observation and its semantic outbox row MUST commit atomically in SQLite. Qdrant writes occur asynchronously.

A source generation publishes only after its required durable and semantic staging conditions pass according to the ledger contract.
