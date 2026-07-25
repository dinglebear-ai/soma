# Performance and Scale Plan

## Principle

Set budgets from representative appliance workloads, then prevent regressions. Do not claim unlimited scale.

## Workloads

### Knowledge

- small repository;
- monorepo;
- large documentation crawl;
- refresh with 1 percent changed content;
- mixed source adapter batch.

### Observations

- steady low-volume homelab;
- burst syslog;
- high-volume Docker logs;
- OTLP batch;
- heartbeat/inventory cadence;
- semantic projection backlog.

### Query

- exact FTS;
- semantic knowledge;
- recent observation search;
- entity-local graph;
- temporal investigation;
- mixed GraphRAG synthesis.

## Measurements

- records/documents/chunks per second;
- queue and outbox lag;
- SQLite transaction latency;
- WAL/checkpoint behavior;
- TEI throughput/utilization;
- Qdrant upsert/search latency;
- graph traversal latency by depth;
- context broker lane and total latency;
- CPU, RSS, disk, network;
- storage growth by canonical/derived class;
- recovery time after service outage;
- rebuild time.

## Initial regression policy

Until absolute budgets are calibrated:

- no statistically significant throughput regression above the approved tolerance against the pinned benchmark host;
- no unexplained memory growth across soak;
- canonical ingestion remains available while TEI/Qdrant are down;
- query timeouts are bounded;
- all result sets and graph traversals enforce limits;
- restart recovery catches up without unbounded backlog amplification.

## Soak

Run at least one long-duration mixed workload before v1 release, including:

- continuous observations;
- scheduled source refresh;
- crawl;
- semantic projection;
- GraphRAG queries;
- backup;
- retention cleanup;
- provider restart.
