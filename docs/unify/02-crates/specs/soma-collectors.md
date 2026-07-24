# `soma-collectors`

**Proposed path:** `crates/shared/observations/collectors`  
**Delivery phase:** Observation ingestion  
**Publication:** Publishable with no default heavy adapters.

## Purpose

Feature-gated receivers, collectors, and normalizers for Cortex's operational sources.

## Donor material

- Cortex: receiver/syslog
- Cortex: filetail
- Cortex: docker_ingest
- Cortex: OTLP
- Cortex: heartbeat
- Cortex: inventory
- Cortex: shell history and AI-session operational evidence

The donor implementation is a behavioral reference, not the public API. Product names, environment variables, database rows, transport DTOs, and current internal dependency seams MUST be removed or adapted.

## Responsibilities

- RFC3164/RFC5424/CEF syslog
- UDP/TCP receivers and sender optional
- Managed file tails
- Docker stdout/stderr and lifecycle events
- OTLP log conversion and optional HTTP receiver
- Heartbeat host snapshots
- Inventory collectors/interfaces
- Shell-history parsing
- AI-session observation projection
- ANSI/severity/template normalization

## Explicit exclusions

- Canonical store
- RAG/vectorization
- Soma configuration/env vars
- Fleet deployment
- API/MCP surfaces

## Public API candidates

- `ObservationAdapter`
- `Receiver`
- `Collector`
- `SyslogCodec`
- `FileTailSource`
- `DockerNormalizer`
- `OtlpConverter`
- `HeartbeatCollector`
- `InventoryCollector`
- `ShellHistoryParser`

Public APIs MUST use crate-owned types or types from a lower-layer shared crate. `anyhow::Error`, Axon/Cortex database rows, and Soma product DTOs MUST NOT appear in the public boundary.

## Dependencies

- soma-observations
- soma-ingest
- soma-transcript
- soma-sanitize
- soma-process optional
- bollard/opentelemetry/tree deps by feature

## Feature plan

- `syslog`
- `file-tail`
- `docker`
- `otlp`
- `heartbeat`
- `inventory`
- `shell-history`
- `sessions`
- `journald`
- `all`

Default features MUST remain minimal. Heavy providers, storage engines, platform collectors, and parser grammars are opt-in unless an ADR approves otherwise.

## Required behavior

1. All limits, clocks, paths, policies, and provider handles are explicit inputs.
2. Cancellation behavior is documented and tested.
3. Error classification distinguishes transient, permanent, invalid-input, unavailable-provider, and cancelled states where applicable.
4. Diagnostics are bounded and secret-safe.
5. Stable identifiers and serialized records have golden compatibility fixtures.
6. Implementations remain usable without Soma's CLI, API, MCP, web server, or global configuration.

## Verification

- wire-format fixtures
- rotation/truncation
- Docker events
- OTLP protobuf fixtures
- heartbeat platform tests
- inventory partial success
- history redaction
- feature matrix

## Initial Soma consumers

- Soma observation runtime

## Extraction acceptance

```text
[ ] Donor paths and exact source baseline recorded
[ ] Neutral API accepted
[ ] Donor fixtures copied or recreated
[ ] Pure implementation moved
[ ] Product/config dependencies removed
[ ] Optional backend adapters implemented
[ ] Soma integration proves real use
[ ] External consumer fixture passes
[ ] Package contents reviewed
[ ] Publication gate passes
```

## Deferred work

Features not required by a v1 vertical slice remain deferred rather than represented by placeholder public APIs. The crate MUST NOT add APM, worker-agent, Incus mission, or Orchestrator concepts.
