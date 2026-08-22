---
title: "Soma Agent Runtime Implementation Plan"
created: 2026-08-05
updated: 2026-08-05
doc_type: "implementation-plan"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
upstream_refs:
  - "BASELINES.md"
  - "CODE-MAP.md"
  - "implementation/FILE-PLAN.md"
  - "implementation/CODE-SKELETONS.md"
  - "implementation/TEST-MATRIX.md"
last_reviewed: "2026-08-05"
---

# Implementation Plan

## Goal

Deliver the example read-only incident investigator as a complete vertical slice using Soma's current application architecture, shared Code Mode engine, gateway, local Incus client, Codex app-server client, Axon retrieval/jobs/synthesis behavior, Cortex observations/evidence, LABBY snippet and gateway behavior, and APM package contracts.

The first release is deliberately narrow:

- read-only;
- one-shot;
- one service;
- Codex app-server only;
- local Incus Unix socket only;
- logical LABBY loadout only;
- dependent Axon research depth one;
- full lifecycle observation through Cortex-compatible contracts.

## Non-negotiable implementation rules

1. Extend <code>SomaApplication</code> and <code>ApplicationPorts</code>; do not add a parallel application facade.
2. Construct concrete adapters only in <code>apps/soma/src/bootstrap.rs</code>.
3. Reuse <code>soma-codemode</code>; do not create another JavaScript runner.
4. Reuse <code>soma-incus-client</code>; do not shell out to Incus for covered operations.
5. Reuse <code>CodexSession</code>; do not create another Codex JSON-RPC client.
6. Transplant product-neutral Axon, Cortex, and LABBY behavior into shared/product boundaries instead of runtime-calling donor repositories.
7. Keep APM as a process-backed package manager; do not port its resolver into Rust first.
8. Preserve context-v1 canonical storage, evidence, graph, retrieval, citations, and surface architecture.
9. Every side effect must have durable run state, idempotency, cancellation, and lifecycle evidence.
10. Do not begin resident assistants or mutation workflows before the read-only slice is verified.

## Plan documents

- [PHASES-00-05.md](implementation/PHASES-00-05.md): contracts, paths, application boundaries, durable runs, snippets, context compilation.
- [PHASES-06-10.md](implementation/PHASES-06-10.md): materialization, LABBY loadouts, Incus, Codex, disclosure.
- [PHASES-11-15.md](implementation/PHASES-11-15.md): Cortex lifecycle, Code Mode context, Axon synthesis, APM, surfaces and E2E.
- [FILE-PLAN.md](implementation/FILE-PLAN.md): exact source files to add or edit.
- [CODE-SKELETONS.md](implementation/CODE-SKELETONS.md): near-compilable code for the principal seams.
- [TEST-MATRIX.md](implementation/TEST-MATRIX.md): unit, integration, security, recovery, and E2E verification.

## Baseline procedure

Before each implementation branch:

~~~bash
git fetch origin --prune
git status --short --branch
git rev-parse HEAD
git rev-parse origin/main
~~~

Compare the result with [BASELINES.md](BASELINES.md). Any donor baseline change requires a focused re-audit of the cited paths and an update to [PROGRESS.md](PROGRESS.md).

## Pull-request sequence

Keep each pull request independently useful and reversible:

1. contracts, schemas, fixtures, generated checks;
2. paths and config;
3. domain types and application ports;
4. durable run control;
5. shared Code Mode snippet store;
6. context validation, compile, and store;
7. context materializers;
8. LABBY loadout adapter;
9. Incus workload operations;
10. Codex assistant adapter;
11. progressive disclosure;
12. lifecycle events and Cortex adapter;
13. run-scoped context-aware Code Mode;
14. dependent Axon research and synthesis;
15. APM adapter;
16. CLI, API, MCP, web, and full vertical-slice verification.

## Vertical-slice acceptance flow

~~~text
validate stack and imported schemas
-> verify APM manifest, lock, audit, and package inventory
-> validate and compile context manifest
-> resolve and pin LABBY loadout
-> create Incus instance locally
-> transfer bootstrap and mount repository/docs/context/artifacts
-> start Soma supervisor and Codex app-server
-> disclose bootstrap capsule
-> execute trace-service-failure snippet through scoped Code Mode
-> create one dependent Axon research job when evidence requires it
-> compile a child context generation
-> emit structured synthesis and briefing
-> verify schemas and evidence links
-> publish run manifest and Cortex lifecycle timeline
-> stop, snapshot, or delete the instance according to policy
~~~

A run is successful only when declared outputs validate and the lifecycle reaches a clean terminal state. A model response by itself is not completion.

## Commands required in every implementation PR

At minimum:

~~~bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p <changed-crate>
cargo test -p soma --test architecture_boundaries
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --no-deps
cargo xtask generated check
git diff --check
~~~

Add the focused commands from [TEST-MATRIX.md](implementation/TEST-MATRIX.md). Runtime phases must also include one safe live invocation through Soma and LABBY, plus Incus state verification where applicable.

## Completion gate

The runtime is ready for the next horizon only when AR-15 in [PROGRESS.md](PROGRESS.md) is verified with:

- exact package and context inputs;
- server-side scoped gateway catalog;
- local Incus isolation;
- Codex session and approval evidence;
- progressive disclosure receipts;
- Code Mode snippet execution;
- dependent Axon research;
- child context generation;
- structured cited synthesis;
- Cortex lifecycle and graph correlation;
- output verification;
- cleanup or retained snapshot evidence.

Resident assistants, physical gateways, multiple services, custom images, broad mutation, and remote Incus remain separate later programs.
