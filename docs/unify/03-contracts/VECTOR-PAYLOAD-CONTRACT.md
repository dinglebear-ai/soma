# Vector Collection and Payload Contract

## Collections

Initial physical collections:

```text
soma_knowledge_v1
soma_observations_v1
soma_memory_v1
soma_graph_v1
```

The context broker hides collection layout from callers.

## Required payload fields

Every point contains:

```text
point_kind
canonical_refs
subject_ids
projection_version
content_hash
embedding_profile
retention_class
sensitivity
created_at
```

Knowledge points also carry source/document/generation identities.

Observation points carry time range, observation kinds, severity summary, and service/host/run identifiers where known.

Memory points carry memory status/scope.

Graph points carry entity/relationship/community identities.

## Prohibited payload

Payload MUST NOT include:

- credentials;
- authorization headers;
- raw browser state;
- unbounded raw log batches;
- the only copy of canonical content;
- product-private fields not required for filtering/hydration.

A safe bounded excerpt MAY be stored.

## Named vectors

A collection MAY use:

- dense semantic vector;
- sparse learned lexical vector;
- additional model-specific vectors when justified.

Vector names and dimensions are collection schema.

## Filtering

Payload indexes SHOULD cover:

- point kind;
- source/subject/entity IDs;
- generation;
- time range;
- severity;
- sensitivity;
- retention class;
- projection version.

## Publication and visibility

Knowledge points are staged under a source generation. Query visibility follows the authoritative ledger generation.

Observation points become visible after successful projection but always hydrate from canonical evidence.

## Reindex

A new embedding or projection version writes new point identities or collection generation, validates retrieval, switches product configuration, then deletes old points asynchronously.

## Dead evidence

A cleanup worker deletes or rewrites points whose canonical references are no longer resolvable. Context results MUST never return uncited orphan points as factual evidence.
