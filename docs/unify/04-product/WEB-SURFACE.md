# Aurora Web Product Surface

The existing mock web application is the shell for the complete product. V1 adds context domains without replacing gateway pages.

## Navigation

```text
Overview
Gateway
├── Providers
├── Upstream MCP Servers
├── Tools
├── Prompts
└── Resources

Knowledge
├── Sources
├── Documents
├── Crawls
└── Indexes

Observability
├── Live Events
├── Logs
├── Timelines
├── Hosts & Services
└── Ingestion Health

Context
├── Search
├── Investigations
├── Graph Explorer
└── Citations

Memory
├── Active
├── Review
└── History

Operations
├── Jobs
├── Storage & Retention
├── Reindex
├── Health
└── Settings
```

## Required v1 pages

### Context search

- natural-language query;
- mode selection or `auto`;
- source/entity/time filters;
- strategy visualization;
- result groups by knowledge, observations, graph, and memory;
- citations and exact canonical record drawers;
- optional synthesized answer with claim classifications;
- query plan and timings for operator users.

### Investigation view

- subject entities;
- deployed/current state;
- timeline;
- relevant logs;
- relevant source/configuration/documentation;
- graph paths;
- findings and evidence gaps;
- saved investigation artifact.

V1 creates investigation results. It does not autonomously dispatch an implementation agent.

### Source management

- type/adapter;
- canonical origin;
- latest committed generation;
- refresh state;
- item/document/chunk counts;
- last error;
- refresh/watch policy;
- run history;
- storage and reindex actions.

### Observation explorer

- FTS query;
- time range;
- host/service/source/severity filters;
- surrounding-event expansion;
- live tail where supported;
- canonical record detail;
- semantic projection status.

### Graph explorer

- entity search;
- aliases;
- neighborhood/path;
- evidence for every edge;
- authority/trust/confidence;
- time slider;
- projection version.

### Jobs

- queue/running/history;
- stage and progress;
- events;
- cancellation/retry;
- linked source/receiver/projection.

### Storage

- per-database and collection usage;
- retention policies;
- semantic backlog;
- dead-letter counts;
- last backup;
- rebuild actions.

## Shared UI states

Every page uses standardized Aurora components for:

- initial loading;
- incremental progress;
- empty state;
- partial success;
- retryable failure;
- permanent configuration error;
- unauthorized/forbidden;
- stale projection;
- degraded provider;
- destructive confirmation.

## Accessibility and scale

- virtualize large result/timeline tables;
- preserve keyboard navigation;
- provide text alternatives for graph visualization;
- never require color alone to distinguish authority or severity;
- paginate canonical records;
- stream bounded progress rather than entire log stores.
