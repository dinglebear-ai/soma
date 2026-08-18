---
title: "Cortex Extraction Source Inventory"
created: 2026-08-17
updated: 2026-08-17
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-17"
---

# Cortex Extraction Source Inventory

## Donor

Reviewed repository: `/home/jmagar/workspace/cortex`.

Donor commit: `7edf23fadb94650c2d2a2f9c80111fb44319eea8` on
`codex/graph-projection-lifecycle`. The donor was clean at inventory time and
was two commits ahead of `origin/main`:

- `7edf23fa fix: keep investigation graph projection current`
- `f9d066e2 fix: make Cortex correlation evidence useful`

Future lanes must rebase their source inventory deliberately if Cortex advances.
Do not mix files from different donor commits without recording the change.

## Existing product shape

Cortex currently ships as one package and binary, but its own architecture guide
identifies three operational sub-products sharing SQLite and a service layer:
log intelligence, fleet/investigation, and deployment tooling. That existing
shape is the starting point for extraction, not something invented by this plan.

## Source-to-target map

| Current Cortex source | Planned owner | Notes |
| --- | --- | --- |
| `src/normalize.rs` | `cortex-ingest-core` | Wave 0 proof. Pure scanner + signature hash. |
| `src/ingest_metadata.rs` | `cortex-ingest-core` | Wave 0 proof. Bounded/redacted metadata. |
| Pure parser/enrichment contracts in `src/enrich/**` | `cortex-ingest-core` or `cortex-ingest` | Move only pieces that do not require storage rows; current output/dispatch paths depend on `db::LogBatchEntry`. |
| `src/app/models/**` | `cortex-domain` | Requires decoupling first because many public models currently embed/convert from `db::*`, inventory, scanner, filetail, and runtime counters. |
| `src/app/error.rs`, request/invariant types | `cortex-domain` | Move stable semantic errors/contracts after dependency audit. |
| `src/db.rs`, `src/db/**` | `cortex-storage-sqlite` | Pool, migrations, FTS/query layer, incident/event storage, graph projections, maintenance state. |
| `src/receiver.rs`, `src/receiver/**` | `cortex-ingest` | Syslog listeners, parsing, supervision; runtime lifecycle should be injectable. |
| `src/ingest.rs` | `cortex-ingest` | Batch writer should target a sink contract; storage adapter implements it. |
| `src/otlp.rs`, `src/otlp/**` | `cortex-ingest` | OTLP source/HTTP adapter, feature-gated from core pipeline. |
| `src/docker_ingest.rs`, `src/docker_ingest/**` | `cortex-ingest` | Docker source adapter, feature-gated. |
| `src/filetail.rs`, `src/filetail/**` | `cortex-ingest` | File source adapter; path/state contracts separated from product config. |
| `src/shell_history_ingest.rs`, `src/ai_transcript_ingest.rs`, `src/ai_watch/**`, `src/scanner/**` | `cortex-ingest` | Host/transcript sources and scanning. Scanner result types used by public models need a domain seam. |
| `src/inventory.rs`, `src/inventory/**` | `cortex-inventory` | Collectors, schemas, redaction, cache, orchestration. |
| `src/heartbeat.rs` | `cortex-inventory` plus domain contracts | Central heartbeat ingest/query semantics belong with inventory; transport parsing stays at adapter edge. |
| `src/agent_observatory.rs`, `src/agent_observatory/**` | `cortex-observatory` | Identity, attribution, classification, lifecycle, projector sources. Persistence edges need ports. |
| `src/git_observer/**`, command/skill/hook/MCP observation pieces | `cortex-observatory` | Group by observation semantics, not current file location. |
| `src/agent.rs`, `src/agent/**`, `src/heartbeat_agent.rs`, `src/agent_deploy.rs` | `cortex-agent` | Host-local runtime and forwarding. Keep independent of central server DB. |
| `src/app.rs`, `src/app/services/**`, assessment/correlation logic | `cortex-application` | Existing CortexService is the seed application facade. |
| `src/api.rs`, `src/api/**` | `cortex-api` | Thin REST adapter over application. |
| `src/mcp.rs`, `src/mcp/**` | `cortex-mcp` | Thin MCP adapter over application; auth migrates to Soma shared auth. |
| `src/config.rs`, `src/runtime.rs`, `src/runtime/**`, `src/logging.rs`, runtime observability/notifications composition | `cortex-runtime` | Typed capability config may live lower; env precedence and process assembly stay here. |
| `src/compose.rs`, `src/compose/**`, `src/setup.rs`, `src/setup/**`, `src/deploy.rs`, `src/doctor.rs`, `src/update.rs` | `cortex-ops` | Local operational mechanics and reusable setup/doctor/update behavior. |
| `src/cli.rs`, `src/cli/**`, `src/web_app.rs`, `src/main.rs` | `apps/cortex` plus thin surface crates | Main is currently large and owns dispatch; final binary should compose library APIs. |

## Coupling hotspots discovered

### Public models depend upward/downward at once

A rough inventory found about 262 public application/model items. The model layer
is not yet safe to copy into a domain crate. Examples include:

- `app/models/log_query.rs` importing inventory schemas and DB result types;
- `app/models/context.rs`, `stats.rs`, `graph.rs`, AI session/event models,
  and RAG models implementing conversions from `db::*` rows;
- `app/models/stats.rs` reading a receiver runtime counter;
- `app/models/core.rs` exposing scanner and heartbeat DB types;
- `app/models/ops.rs` re-exporting file-tail implementation types and using
  application errors directly;
- surface models exposing notification DB rows and runtime configuration.

The correct first domain lane is therefore an untangling lane: move semantic
types, relocate database conversions to storage/application adapters, and remove
implementation types from public responses before moving the directory.

### Enrichment depends on storage batches

Core parsers are close to reusable, but `enrich/output.rs` and
`enrich/dispatch.rs` currently operate directly on `db::LogBatchEntry`. The
future ingest boundary should introduce a transport/storage-neutral event/batch
type or sink contract instead of moving the database type into a lower crate.

### Authentication is externally pinned

The Cortex package currently depends on `lab-auth` by Labby git revision.
References exist across runtime, MCP routes/server/tools, OTLP auth, heartbeat,
agent/shell/transcript ingestion, test helpers, and OAuth integration tests. The
extraction contract requires migration to Soma's `soma-auth` shared crate or a
shared adapter while preserving OAuth/resource/scope parity.

### Binary composition is thick

`src/main.rs` owns substantial command dispatch and runtime decisions. The
final `apps/cortex` target should retain parsing/mode selection while moving
reusable runtime/business behavior behind library APIs and a runtime builder.

## Wave 0 exact source claim

`cortex-ingest-core` initially claims only:

- donor `src/normalize.rs` and `src/normalize_tests.rs`;
- donor `src/ingest_metadata.rs` and `src/ingest_metadata_tests.rs`.

The only intended source-level changes in that proof are public visibility,
public documentation, the metadata test sidecar filename, and crate integration.
No normalization/redaction algorithm change is part of wave 0.
