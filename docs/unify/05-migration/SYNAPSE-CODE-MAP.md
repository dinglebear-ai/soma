---
title: "Synapse Code Map and Extraction Ledger"
created: 2026-08-01
updated: 2026-08-01
status: normative
---

# Synapse Code Map and Extraction Ledger

## Purpose

This is the authoritative map from the pinned Synapse donor to Soma shared crates and retained product code. It prevents blind copying, duplicate implementations, and leakage of Synapse policy into reusable crates.

## Baseline

- Donor: `dinglebear-ai/synapse`
- Branch: `main`
- Reviewed commit: `8f1bb2ef`
- Target: `dinglebear-ai/soma`
- Stack base: `feat/product-family-architecture`
- Implementation branch: `feat/operations-foundation`
- Canonical donor surface: 59 operations across Flux, Scout, and shared help

The donor commit MUST be pinned in `donors.lock.toml`. Any refresh requires fixture regeneration, semantic diffing, and explicit disposition of every changed behavior.

## Ownership boundaries

### soma-ops

Owns canonical operation identity, typed targets, safety classifications, request envelopes, deadlines, cancellation semantics, idempotency, opaque authorization evidence, planning, fingerprints, progress, results, verification, diagnostics, artifacts, evidence, and lifecycle events.

It MUST NOT import RMCP, Axum, Bollard, SSH process implementations, product scopes, product environment variables, or product configuration.

### soma-fleet

Owns host identity and topology, host repository and resolver traits, SSH execution, connection pooling, known-host verification, forwarding, transfer, shutdown, fanout admission, concurrency, deadlines, cancellation propagation, stale-connection invalidation, partial success, and per-target evidence.

It MUST NOT own Docker, Compose, ZFS, or filesystem business semantics.

### soma-infra

Owns typed infrastructure operations and drivers for Docker, containers, Compose, hosts, filesystems, processes, logs, ZFS, transfer, and later Incus. It depends on soma-ops and may consume soma-fleet adapter traits.

It MUST NOT parse Flux or Scout JSON envelopes.

### Synapse product code

Retains Flux and Scout names and aliases, JSON parsing, compatibility errors, product scopes, CLI grammar and formatting, confirmation UX, MCP tools and schemas, prompts, resources, elicitation, REST compatibility, standalone auth, setup, doctor, web hosting, product config precedence, activity counters, and process composition.

## Donor module ledger

| Donor path | Target | Extract | Retain in Synapse | Proof gate |
|---|---|---|---|---|
| `src/actions/operations.rs` | soma-ops + adapter | canonical operation semantics | Flux/Scout ownership, REST exposure, scopes, legacy field names | all 59 semantic entries match |
| `src/actions.rs` | soma-infra + adapter | typed requests and enums | action/subaction parsing and compatibility errors | normalized-request differential tests |
| `src/actions/flux.rs` | soma-infra | Docker, container, Compose, host requests | Flux vocabulary | request fixture parity |
| `src/actions/scout.rs` | soma-infra | file, process, log, ZFS, exec, transfer requests | Scout vocabulary | request fixture parity |
| `src/actions/dispatch.rs` | Synapse facade | none initially | compatibility dispatch and response wrapping | all dispatch tests unchanged |
| `src/flux_service/docker*` | soma-infra::docker | engine inspection semantics | legacy output projection | golden result parity |
| `src/flux_service/container*` | soma-infra::container | read and lifecycle operations | legacy defaults and formatting | plan, mutation, verification tests |
| `src/flux_service/compose*` | soma-infra::compose | discovery, status, logs, pull, up/down/restart/recreate | legacy lookup and output | fixture and live Compose smoke |
| `src/flux_service/host*` | soma-infra::host | host inspection and safe host operations | display conventions | driver conformance |
| `src/scout_service/fs*` | soma-infra::fs | stat, list, read, tail, hash, mkdir, write, remove, move, copy | Scout aliases | secure-path regression suite |
| `src/scout_service/proc*` | soma-infra::process | list and inspect | output formatting | fixture parity |
| `src/scout_service/logs*` | soma-infra::logs | bounded reads and filtering | Scout envelope | output-bound tests |
| `src/scout_service/zfs*` | soma-infra::zfs | pool, dataset, snapshot operations | availability messaging | capability tests |
| `src/scout_service/exec*` | soma-infra::exec + soma-fleet | structured local/remote execution | Scout command syntax | injection and deadline tests |
| `src/docker_client/*` | soma-infra::docker::driver | driver trait, Bollard adapter, mock, cache | product construction | mock/live conformance |
| `src/compose.rs` | soma-infra::compose | project models and validation | product search paths | discovery parity |
| `src/host_config.rs` | soma-fleet + Synapse config | host record, repository, resolver | env/file/SSH precedence | topology revision tests |
| `src/ssh/*` | soma-fleet::ssh | pool, known hosts, forwarding, transfer | product config loading | security and remote smoke tests |
| `src/fanout.rs` | soma-fleet::fanout | bounded fanout and partial results | CLI presentation | deterministic partial-success tests |
| `src/runtime_budget.rs` | soma-ops + soma-fleet | deadline model and enforcement split | product defaults | fake-clock tests |
| `src/secure_path.rs` | soma-infra::fs | descriptor-confined path policy | allowed-root config | traversal and symlink-race tests |
| `src/elicitation_gate.rs` | soma-ops + adapters | evidence structure and binding | MCP elicitation, CLI prompt, deny policy | binding and SEP-2260 tests |
| `src/cache.rs` | private owner | no initial extraction | all | second-consumer proof required |
| `src/app.rs`, `config.rs`, `activity.rs`, `scaffold.rs` | Synapse product | no initial extraction | product lifecycle and configuration | standalone E2E |
| `src/mcp/*`, `token_limit.rs` | Synapse MCP | none | complete MCP compatibility | schema and mcporter tests |
| `src/api.rs`, `server*`, `web.rs` | Synapse API | none | REST, auth, health, readiness, web | OpenAPI and route tests |
| `src/cli/*`, `formatters/*` | Synapse CLI | none | grammar, output, setup, doctor, watch | snapshots and destructive smoke |
| `src/main.rs` | apps/synapse | composition only | mode dispatch and shutdown | process smoke |

## Dependency direction

`soma-ops <- soma-fleet <- soma-infra <- product adapters`

Allowed:

- local-only soma-infra drivers may depend directly on soma-ops;
- Synapse compatibility crates may consume all three shared crates;
- products inject policy, drivers, clocks, sinks, and authorization verifiers.

Forbidden:

- shared crates depending on Synapse or Soma product crates;
- soma-ops depending on fleet or infrastructure;
- infrastructure importing MCP, REST, or CLI request types;
- independently maintained operation catalogs.

## Extraction method

1. Characterize donor behavior before movement.
2. Generate semantic fixtures from the pinned donor.
3. Introduce typed models beside the donor implementation.
4. Build compatibility adapters from donor envelopes to shared requests.
5. Run old and new implementations in differential tests.
6. Cut one vertical slice at a time.
7. Delete donor runtime code only after standalone Synapse consumes the shared path.
8. Mark each donor file extracted, retained, deferred, or retired.

## Completion rule

Extraction is complete only when standalone Synapse is built from Soma shared crates, all 59 operations pass semantic parity, shared crates contain no product configuration reads, and no independent donor runtime remains capable of drifting.
