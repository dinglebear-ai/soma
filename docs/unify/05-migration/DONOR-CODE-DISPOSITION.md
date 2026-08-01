---
title: "Donor Code Disposition"
created: 2026-07-24
updated: 2026-07-30
---

# Donor Code Disposition

This document prevents accidental one-for-one copying.

## Axon

| Donor crate | Target | Disposition |
|---|---|---|
| axon-adapters | soma-sources; soma-crawl; Soma knowledge runtime | Split adapter SDK/source implementations from crawling and product orchestration. |
| axon-api | soma-primitives plus domain-owned types; Soma API facade | Dissolve. Do not recreate a giant shared API crate. |
| axon-authz | Existing Soma auth/application policy | Do not migrate surface/product scope vocabulary. |
| axon-cli | Existing Soma CLI | Do not migrate. |
| axon-core | soma-sanitize; soma-process; crate-owned types; Soma config/runtime | Dismantle. No `soma-core` dumping ground. |
| axon-document | soma-rag | Port preparation, routing, chunking, citations and processing identity. |
| axon-embedding | soma-rag | Port provider contracts/batching; keep concrete providers optional. |
| axon-error | crate-owned errors plus soma-primitives diagnostics | Port classifications, not one universal error enum. |
| axon-extract | soma-sources; soma-crawl; soma-rag structured normalization | Split vertical extraction behavior by responsibility. |
| axon-graph | soma-graph | Merge with Cortex evidence/trust/temporal behavior. |
| axon-jobs | soma-jobs | Purify from adapters, LLM, ledger, graph, and product runners. |
| axon-ledger | soma-ledger | Direct strong extraction with neutral DTOs. |
| axon-llm | soma-llm | Reuse existing Soma Codex client through an adapter. |
| axon-mcp | Existing Soma MCP | Do not migrate. |
| axon-memory | soma-memory | Extract after RAG/context contracts stabilize. |
| axon-observe | Existing Soma observability plus job/provider metrics | No replacement crate. |
| axon-parse | soma-rag | Port parser registry, facts, code/structured parsing and graph candidates. |
| axon-prune | soma-ledger cleanup debt plus Soma retention use cases | Do not create a separate crate initially. |
| axon-retrieval | soma-rag plus Soma context broker | Port hybrid mechanics; graph-aware orchestration belongs to product broker. |
| axon-route | soma-route | Direct extraction. |
| axon-services | Soma application/runtime knowledge modules | Mine live orchestration; do not publish `axon-services` under a new name. |
| axon-vectors | soma-rag | Port vector contracts, payload policy, BM42/hybrid and Qdrant adapter. |
| axon-web | Existing Soma Aurora web application | Do not migrate. |


## Cortex

| Donor area | Target | Disposition |
|---|---|---|
| src/ingest.rs; src/db/* | soma-observations; soma-ingest | Canonical models, writer, FTS, batching, retention and query behavior. |
| src/ingest_metadata.rs | soma-sanitize | Bounded metadata and redaction. |
| src/receiver/* | soma-collectors | Syslog codec/receiver and peer-vs-claimed identity. |
| src/filetail/* | soma-collectors; soma-ingest | File identity, rotation/truncation, checkpoints and source supervision. |
| src/docker_ingest/* | soma-collectors | Docker logs/events normalization. |
| src/otlp/* | soma-collectors | OTLP log conversion and optional receiver. |
| src/heartbeat* | soma-collectors | Host telemetry snapshots and collection contracts. |
| src/inventory/* | soma-collectors; soma-graph | Inventory models/collectors and topology projection. |
| src/shell_history_ingest.rs; src/command_log.rs | soma-collectors | Shell history and command evidence. |
| src/scanner/*; src/ai_*; src/agent_command_ingest.rs | soma-transcript; source/observation adapters | One typed session model with document and operational projections. |
| src/enrich/*; src/normalize.rs | soma-collectors | Log parsing, templates, severity and enrichment. |
| Cortex investigation graph tables/contracts | soma-graph | Trust, confidence, temporal evidence, resolver and explanation paths. |
| src/assessment/* | soma-llm or later Soma product analysis | Only generic provider execution/contracts migrate in v1. |
| src/compose/* | observation adapters and product inventory/config snapshot modules | Ingest and graph active Compose state; mutation/repair is not a v1 context requirement. |
| src/deploy/*; src/agent_deploy* | Existing Soma self-update or deferred product operations | Not part of Agent/Incus work and not required to merge context planes. |
| src/update.rs | Existing soma-self-update adoption | No new extraction. |
| src/notifications/* | Existing integrations or later notification product work | Not required for v1 context merger. |
| src/api/*; src/mcp/*; src/cli/*; src/surfaces/*; src/web_app.rs | Existing Soma surfaces | Do not migrate. |
| src/config.rs; src/app/*; src/runtime/*; src/setup/* | Soma product config/runtime composition | Mine behavior only where needed. |
| src/logging/*; src/observability* | Existing Soma observability and traces | Do not create a competing stack. |


## Synapse

| Donor area | Target | Disposition |
|---|---|---|
| src/actions/operations.rs | soma-ops | Extract operation identity, target, safety, parameter, planning/progress/verification capabilities, and event metadata. Product scopes remain in Synapse. |
| src/actions.rs; src/actions/flux.rs; src/actions/scout.rs | soma-infra plus Synapse application | Move typed domain requests into the engine; preserve Flux/Scout JSON parsing as compatibility adapters. |
| src/actions/dispatch.rs | Synapse application initially | Preserve legacy action dispatch while delegating behavior to typed shared engines. |
| src/flux_service.rs; src/flux_service/* | soma-infra | Docker, container, Compose, and host operation semantics. |
| src/scout_service.rs; src/scout_service/* | soma-infra | File, process, log, ZFS, exec, fanout-command, and transfer semantics. |
| src/docker_client/*; src/docker.rs; src/compose.rs | soma-infra | Docker traits, Bollard adapter, client cache, Compose discovery, and validation. |
| src/host_config.rs | soma-fleet plus Synapse application | Extract explicit host models and repository traits; retain environment/file/SSH precedence as product policy. |
| src/ssh/*; src/fanout.rs | soma-fleet | OpenSSH pool, forwarding, known-host behavior, transfer, fanout, deadlines, shutdown, and security fixtures. |
| src/runtime_budget.rs | soma-ops; soma-fleet | Generic deadline contract in ops; transport enforcement in fleet. |
| src/secure_path.rs | soma-infra | Keep descriptor-confined path policy beside file operations. |
| src/elicitation_gate.rs | soma-ops plus product adapters | Extract opaque authorization-evidence contract; keep MCP/CLI prompting and product confirmation implementations outside shared crates. |
| src/cache.rs | private owner | Do not publish a generic cache crate until a second independent consumer proves the boundary. |
| src/app.rs; src/config.rs; src/scaffold.rs; src/activity.rs | Synapse product crates | Preserve standalone composition, configuration, setup, doctor, counters, and compatibility facade. |
| src/mcp/*; src/token_limit.rs | Synapse MCP | Preserve Flux/Scout tools, resources, prompts, schemas, elicitation, output bounds, and transports. |
| src/cli/*; src/formatters/*; src/color_policy.rs | Synapse CLI | Preserve command grammar, JSON/human output, confirmation, setup, doctor, and watch behavior. |
| src/api.rs; src/server*; src/web.rs | Synapse API/web | Preserve REST compatibility, auth, health, readiness, status, and lightweight standalone web surface. |
| src/main.rs | apps/synapse | Process composition, mode dispatch, signals, startup, and shutdown only. |


## Existing neutral and Soma foundation

The following are consumed as-is and extended only through their intended contracts:

- the neutral Incus REST client under `crates/shared/incus-client`;
- auth and OAuth;
- gateway;
- provider catalog;
- Code Mode;
- MCP/REST/CLI/OpenAPI/web projection;
- observability and traces;
- self-update;
- current domain/application/runtime composition.

## Rule

A donor path has exactly one disposition:

```text
extract into shared crate
port into Soma product layer
adapt into existing Soma capability
defer
retire/leave behind
```

No donor module may be copied without a ledger entry and a behavioral fixture or explicit waiver.
