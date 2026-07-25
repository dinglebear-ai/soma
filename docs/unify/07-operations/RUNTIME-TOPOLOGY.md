# Runtime Topology

## V1 appliance

A fixed Soma Incus system-container image MAY bundle:

```text
Soma product binary
SQLite databases
Qdrant
TEI
Headless Chrome
supporting certificates/configuration
```

This is not custom per-agent image construction. Agent worker containers are outside v1.

## Public boundary

Only Soma exposes the public listener:

```text
public port
    ↓
Soma HTTP server
├── /api/v1/*
├── /mcp
├── /oauth/*
├── /ui/*
├── /openapi.json
└── /health/*
```

Qdrant, TEI, Chrome, and internal workers bind loopback or Unix sockets.

## Product processes

```text
Soma main process
├── application/runtime
├── gateway/provider runtime
├── source/job workers
├── observation receivers/collectors
├── semantic outbox workers
├── graph projection workers
└── web/API/MCP surfaces

Internal services
├── Qdrant
├── TEI
└── Chrome/browser service
```

## Lifecycle

Startup order:

1. load/migrate product configuration;
2. open/migrate canonical SQLite stores;
3. verify internal provider availability;
4. start job runtime;
5. start observation writer;
6. start receivers/collectors;
7. start projection workers;
8. expose public readiness.

Provider outages may degrade readiness by capability without making canonical observation ingestion unavailable.

## Shutdown

1. stop accepting new public mutations;
2. stop new job claims;
3. stop receivers/collectors;
4. drain bounded queues;
5. persist checkpoints;
6. release leases;
7. stop workers;
8. checkpoint/close SQLite;
9. stop internal services as managed by appliance supervisor.

## Health levels

- process liveness;
- public readiness;
- capability health;
- individual source/receiver health;
- dependency health;
- backlog/lag;
- storage pressure.

One failing source does not mark the entire product dead.
