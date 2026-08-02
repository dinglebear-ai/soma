---
title: "Synapse Extraction Plan"
created: 2026-07-31
updated: 2026-08-01
---

# Synapse Extraction Plan

## Goal

Import Synapse into the Soma multi-distribution monorepo while preserving a complete standalone Synapse product and extracting neutral infrastructure engines that Soma can embed directly.

The authoritative detailed artifacts are [`SYNAPSE-EXTRACTION-SPEC.md`](SYNAPSE-EXTRACTION-SPEC.md), [`SYNAPSE-CODE-MAP.md`](SYNAPSE-CODE-MAP.md), [`SYNAPSE-IMPLEMENTATION-PLAN.md`](SYNAPSE-IMPLEMENTATION-PLAN.md), [`../03-contracts/OPERATION-MODELS.md`](../03-contracts/OPERATION-MODELS.md), and [`../03-contracts/OPERATION-SCHEMA.md`](../03-contracts/OPERATION-SCHEMA.md).

## Donor baseline

- Repository: `https://github.com/dinglebear-ai/synapse`
- Branch: `main`
- Commit: `8f1bb2efc1a519c9d3b1b5b41ea8bb2ba178011f`
- Observed: 2026-08-01
- Standalone binary: `synapse`
- Rust package: `synapse2`
- Compatibility MCP tools: `flux` and `scout`
- Canonical operation count at baseline: 59

The extraction lock file MUST pin the full donor commit before source import. Later donor changes require an explicit baseline update.

## Target boundaries

### Neutral shared crates

- `soma-ops`: operation contracts, planning, progress, verification, approvals, and events;
- `soma-fleet`: host topology, SSH, forwarding, transfer, fanout, deadlines, and partial success;
- `soma-infra`: Docker, Compose, Incus, host, file, process, log, and ZFS operations;
- existing `incus-client`: low-level Incus REST client.

### Synapse product crates

- Synapse application: standalone product policy, configuration, Flux/Scout compatibility, setup, and doctor;
- Synapse CLI: command grammar and human/JSON output;
- Synapse MCP: Flux/Scout MCP compatibility, resources, prompts, elicitation, and transports;
- Synapse API: REST compatibility, health, readiness, auth, and standalone web hosting;
- `apps/synapse`: process composition and lifecycle only.

Final public package naming follows the repository's one-word publication rule and requires an ADR-backed fallback when unavailable.

## Donor path map

| Donor area | Target | Disposition |
|---|---|---|
| `src/actions/operations.rs` | `soma-ops` | Extract neutral operation identity, safety, parameter, and capability metadata. Remove product scopes and Flux/Scout ownership. |
| `src/actions.rs`, `src/actions/flux.rs`, `src/actions/scout.rs` | split | Typed domain requests move to `soma-infra`; legacy JSON parsing remains in Synapse compatibility code. |
| `src/actions/dispatch.rs` | Synapse application initially | Preserve compatibility dispatcher while replacing internals with typed shared-engine calls. |
| `src/flux_service*` | `soma-infra` | Docker, container, Compose, and host operations. |
| `src/scout_service*` | `soma-infra` | Files, processes, logs, ZFS, exec, emit, and transfer operations. |
| `src/docker_client*`, `src/docker.rs`, `src/compose.rs` | `soma-infra` | Docker traits, Bollard implementation, caching, discovery, and validation. |
| `src/host_config.rs` | split | Neutral host records and repository traits move to `soma-fleet`; Synapse environment/file precedence remains product policy. |
| `src/ssh*`, `src/fanout.rs` | `soma-fleet` | SSH pool, forwarding, known-host handling, transfer, fanout, lifecycle, and tests. |
| `src/runtime_budget.rs` | `soma-ops` and `soma-fleet` | Generic deadlines in ops; transport enforcement in fleet. |
| `src/secure_path.rs` | `soma-infra` | Descriptor-confined filesystem policy beside file operations. |
| `src/elicitation_gate.rs` | split | Neutral authorization-evidence port in ops; MCP, CLI, deny, and product prompt implementations stay in adapters. |
| `src/cache.rs` | internal owner | Keep private until a second independent consumer proves a public cache boundary. |
| `src/app.rs` | Synapse application | Convert to a compatibility facade over shared engines. |
| `src/config.rs`, `src/scaffold.rs`, `src/activity.rs` | Synapse application/API | Product configuration, setup, runtime counters, and status. |
| `src/mcp*`, `src/token_limit.rs` | Synapse MCP | Preserve the standalone MCP contract. |
| `src/cli*`, `src/formatters*`, `src/color_policy.rs` | Synapse CLI | Preserve CLI compatibility. |
| `src/api.rs`, `src/server*`, `src/web.rs` | Synapse API | Preserve REST, auth, status, readiness, and lightweight web behavior. |
| `src/main.rs` | `apps/synapse` | Configuration, composition, mode dispatch, signal handling, and shutdown only. |

## PR train

The authoritative tasks and exit criteria are in [`SYNAPSE-IMPLEMENTATION-PLAN.md`](SYNAPSE-IMPLEMENTATION-PLAN.md). The stack is:

1. architecture and donor freeze;
2. operations foundation, semantic schema, and generators;
3. history-preserving Synapse product import;
4. canonical catalog and compatibility adapters;
5. fleet foundation;
6. read-only infrastructure engines;
7. mutation framework and infrastructure mutations;
8. standalone Synapse cutover;
9. Soma embedded operations;
10. Incus operations;
11. remote adapter and Labby parity;
12. release, cutover, and donor retirement.

Every PR is developed in its own worktree. Each branch is based on the branch immediately below it until the stack is merged or restacked.

## Initial safety scope

The first Soma integration is read-only inspection and verification. Existing Synapse mutation capabilities remain available in standalone Synapse, but Soma mutation exposure requires the operations authorization and event contracts plus explicit product approval policy.

## Required parity gates

- All 59 baseline operations have a canonical mapping or documented product-only classification.
- Flux and Scout MCP schemas remain compatible until a versioned breaking release.
- CLI help and JSON results pass donor fixtures.
- Auth scope and destructive classifications are preserved in Synapse adapters.
- Shared crates contain no `SYNAPSE_*` or `SOMA_*` configuration reads.
- Shared crates do not depend on RMCP, Axum, product auth, or web frameworks.
- Standalone Synapse and embedded Soma pass independent end-to-end tests.
- Embedded and remote operations adapters pass the same contract suite.
- The security and resource-boundary regressions identified by Synapse's comprehensive review remain enforced.

## Cutover rule

The donor repository remains authoritative until standalone Synapse is built and released from the Soma monorepo with parity, migration, packaging, and rollback evidence. After cutover, the donor repository becomes a generated mirror or archived landing repository; two-way manual synchronization is prohibited.
