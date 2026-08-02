---
title: "V1 Non-Goals"
created: 2026-07-24
updated: 2026-07-31
---

# V1 Non-Goals

## Agent orchestration

V1 defines no `apm.yaml`, `apm.lock`, mission compiler, Orchestrator workflow, implementation agent, reviewer agent, agent monitor plane, or autonomous delivery loop.

## Incus workers

The neutral Incus client remains independently consumable under `crates/shared`, and Synapse is the steward of generic Incus operations. Context v1 still does not:

- create task-specific containers;
- build custom images;
- bake agents, skills, prompts, or MCP servers into images;
- dispatch `/goal`;
- manage worker workspaces.

## Surface redesign

Soma's current gateway, authentication, provider catalog, Code Mode, and integrated surface projection are the source of truth for Soma. Axon, Cortex, and Synapse retain complete standalone surfaces, but context v1 does not migrate those surfaces into Soma as competing frameworks.

## Universal ingestion

V1 does not force every source through one lifecycle. Source generations and observation streams remain distinct.

## Vectorize everything

Routine raw observations are not embedded automatically. Qdrant is not canonical storage.

## New database zoo

V1 does not add Neo4j, Elasticsearch, ClickHouse, Postgres, Redis, RabbitMQ, or Kafka unless a measured blocker is approved through a new ADR.

## Perfect global GraphRAG

Local entity and temporal GraphRAG are required. Hierarchical global communities and DRIFT-style search are designed but may land after v1's core exit criteria.
