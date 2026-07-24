# Existing Surface Integration

## Authority

Soma's current CLI, REST API, MCP, OpenAPI, web, authentication, provider catalog, and Code Mode architecture remain final for v1.

The context layer adds application use cases and projections. It does not import Axon or Cortex surface implementations.

## Projection rule

```text
Shared crates
    ↓
Soma domain/application use cases
    ↓
Existing surface adapters
    ├── CLI
    ├── REST/OpenAPI
    ├── MCP
    └── Aurora web application
```

All surfaces MUST:

- call the same application use case;
- apply the same authorization decision;
- receive the same diagnostic and progress semantics;
- use the same context query contract;
- expose the same canonical IDs and citations.

## MCP

The compact Soma tool remains the default public MCP surface. Context operations appear as actions such as:

```text
context.query
context.investigate
context.entity.resolve
context.entity.neighborhood
knowledge.source.*
observations.search
observations.timeline
memory.recall
jobs.*
```

Code Mode can access the same authorized actions without exposing hundreds of top-level tool schemas.

## REST/OpenAPI

Recommended resource families:

```text
/api/v1/knowledge/sources
/api/v1/knowledge/documents
/api/v1/observations
/api/v1/context/query
/api/v1/context/entities
/api/v1/context/graph
/api/v1/memory
/api/v1/jobs
/api/v1/admin/reindex
/api/v1/admin/retention
```

Long-running operations return job resources.

## CLI

Recommended command families:

```text
soma source ...
soma observations ...
soma context ...
soma graph ...
soma memory ...
soma jobs ...
soma doctor context
```

The CLI is a remote client when pointed at a running Soma instance and MAY support local-only utilities where explicitly designed.

## Web

The web application consumes the REST/client layer and shares Aurora components. It does not query SQLite or Qdrant directly.

## Generated contracts

OpenAPI and MCP action references are generated from application/surface contracts. The context schema bundle remains transport-neutral.
