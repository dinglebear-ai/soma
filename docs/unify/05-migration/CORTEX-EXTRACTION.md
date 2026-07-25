# Cortex Extraction Program

## Goal

Preserve Cortex's reliable operational ingestion and canonical SQLite/FTS behavior, then project selected records into Soma's RAG and evidence graph.

Cortex does not need to adopt Axon's source-generation lifecycle.

## Extraction order

1. observation contract and safe metadata;
2. canonical SQLite/FTS store and batch writer;
3. file-tail or syslog walking skeleton;
4. retention, health, and checkpoints;
5. Docker;
6. OTLP;
7. heartbeat;
8. inventory/configuration snapshots;
9. shell history/command evidence;
10. unified AI-session operational projection;
11. semantic outbox and observation projectors;
12. evidence graph convergence.

## First walking skeleton

```text
file or syslog record
    ↓ adapter
ObservationRecord
    ↓ ingest runtime
SQLite + FTS + semantic outbox
    ↓ query
observations.search/timeline through existing surfaces
```

## Semantic projection

Projection begins selectively:

- AI sessions and run phases;
- MCP/tool calls;
- errors/critical logs;
- stack traces;
- deployment or configuration changes when present;
- incident windows;
- Docker health transitions.

Routine heartbeats, metrics, DNS events, and repeated informational logs remain structured unless policy promotes an anomaly/window.

## Graph projection

Prefer deterministic relationships from:

- host/service/container inventory;
- Compose/configuration snapshots;
- session/tool/MCP records;
- deployment and Docker events;
- canonical source identity.

Model-derived relationships remain lower authority and cite evidence.

## Explicitly not required

- replacing Soma surfaces;
- moving canonical observations to Qdrant;
- making every observation an Axon source;
- Cortex agent deployment;
- autonomous remediation;
- custom Incus images.

## Completion

Cortex extraction is complete when:

- required sources use one observation contract;
- canonical storage/FTS and retention parity pass;
- semantic projection is asynchronous and idempotent;
- graph evidence resolves to canonical rows;
- source/receiver health is visible through Soma;
- Cortex-specific product surfaces are unnecessary.
