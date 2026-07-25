# V1 Non-Goals

## Agent orchestration

V1 defines no `apm.yaml`, `apm.lock`, mission compiler, Orchestrator workflow, implementation agent, reviewer agent, agent monitor plane, or autonomous delivery loop.

## Incus workers

The existing Incus client may remain in Soma, but v1 does not:

- create task-specific containers;
- build custom images;
- bake agents, skills, prompts, or MCP servers into images;
- dispatch `/goal`;
- manage worker workspaces.

## Surface redesign

Soma's current gateway, authentication, provider catalog, Code Mode, and surface projection are the source of truth. Axon and Cortex transports are not migrated as competing frameworks.

## Universal ingestion

V1 does not force every source through one lifecycle. Source generations and observation streams remain distinct.

## Vectorize everything

Routine raw observations are not embedded automatically. Qdrant is not canonical storage.

## New database zoo

V1 does not add Neo4j, Elasticsearch, ClickHouse, Postgres, Redis, RabbitMQ, or Kafka unless a measured blocker is approved through a new ADR.

## Perfect global GraphRAG

Local entity and temporal GraphRAG are required. Hierarchical global communities and DRIFT-style search are designed but may land after v1's core exit criteria.
