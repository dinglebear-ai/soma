---
title: "Cortex Extraction Progress"
created: 2026-08-17
updated: 2026-08-17
doc_type: "report"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-17"
---

# Cortex Extraction Progress

This is the authoritative migration ledger. Check an item only when the stated
evidence exists on the branch or in the linked lane PR.

## Global state

- Donor baseline: `7edf23fadb94650c2d2a2f9c80111fb44319eea8`
- Soma integration branch: `feat/cortex-shared-extraction`
- Working topology: [SPEC.md](SPEC.md)
- Normative rules: [CONTRACTS.md](CONTRACTS.md)
- Verification gates: [VERIFICATION.md](VERIFICATION.md)

## Wave 0: Foundation and proof crate

- [x] Create isolated Soma worktree from freshly fetched `origin/main`.
- [x] Inventory Soma shared-crate conventions and architecture ADRs.
- [x] Inventory Cortex top-level architecture, package dependencies, and coupling hotspots.
- [x] Record immutable Cortex donor commit and remote-main divergence.
- [x] Draft ADR 0014 for Cortex shared-crate extraction.
- [x] Add extraction specification, contracts, source inventory, progress tracker, verification guide, and review log.
- [x] Create `crates/shared/cortex/ingest-core` as the first proof crate.
- [x] Port normalization and bounded metadata donor tests with the implementation.
- [x] Add an external-consumer public API test.
- [x] Wire the proof crate into the Soma workspace and architecture docs.
- [x] Run targeted format, clippy, unit/integration, and rustdoc checks.
- [x] Run Soma architecture and documentation checks.
- [x] Run all-features workspace check and test gates required by ADR 0010.
- [x] Complete architecture/API review and behavior/safety review.
- [x] Resolve every review/test/docs finding and record evidence in REVIEW.md.
- [x] Commit, push, and open extraction-foundation PR #363.

## Wave 1: Domain seam

- [ ] Classify every public `app/models/**` type as semantic contract, storage projection, transport DTO, or runtime state.
- [ ] Introduce `cortex-domain` with only storage/transport-neutral contracts.
- [ ] Move service error taxonomy/invariants that truly belong to domain.
- [ ] Relocate `From<db::...>` mappings out of the domain dependency direction.
- [ ] Remove raw DB, scanner, receiver-counter, filetail, and runtime-config types from public domain responses.
- [ ] Add serialization/parity fixtures for user-visible response models.
- [ ] Add independent consumer tests and README/rustdoc.
- [ ] Pass architecture/all-features gates.

## Wave 2: SQLite storage adapter

- [ ] Create `cortex-storage-sqlite`.
- [ ] Move pool initialization and SQLite configuration.
- [ ] Move migrations with exact migration-order/version parity tests.
- [ ] Move query, FTS, retention, storage-budget, incident/event, graph, and observatory persistence.
- [ ] Implement domain/application repository ports without exposing raw row types upward.
- [ ] Preserve single-writer/maintenance coordination semantics.
- [ ] Add temporary-database consumer fixtures.
- [ ] Pass donor DB suite plus workspace gates.

## Wave 3: Ingest engines

- [ ] Define the reusable ingest event/batch and sink contracts.
- [ ] Create `cortex-ingest` without a hard dependency on the product runtime.
- [ ] Move syslog parsing/listener supervision.
- [ ] Move enrichment parsers and dispatch.
- [ ] Move OTLP ingest behind an optional feature.
- [ ] Move Docker ingest behind an optional feature.
- [ ] Move file, shell-history, transcript, scanner, and watch sources.
- [ ] Preserve backpressure, batching, bounds, redaction, and listener-liveness behavior.
- [ ] Prove an alternate in-memory sink consumer.

## Wave 4: Inventory, observatory, and agent

- [ ] Create `cortex-inventory` and move normalized inventory/cache/collector behavior.
- [ ] Feature-gate service-specific collectors where practical.
- [ ] Create `cortex-observatory` with persistence ports.
- [ ] Move identity, attribution, classification, lifecycle, and projector behavior.
- [ ] Create `cortex-agent` for host-local forwarding/heartbeat/runtime behavior.
- [ ] Prove agent crate does not require the central Cortex SQLite store.
- [ ] Preserve investigation graph and heartbeat contracts.

## Wave 5: Application facade

- [ ] Create `cortex-application`.
- [ ] Move `CortexService` use cases and business policy.
- [ ] Replace concrete lower-layer dependencies with explicit ports/handles where useful.
- [ ] Move correlation, assessment, incident, RAG, maintenance, and map/graph use-case policy.
- [ ] Keep all transport adapters out of the application crate.
- [ ] Port service tests and add mock-port tests for failure paths.

## Wave 6: Product surfaces

- [ ] Create `cortex-api` as a thin REST adapter.
- [ ] Create `cortex-mcp` as a thin RMCP adapter.
- [ ] Migrate auth usage from direct `lab-auth` to `soma-auth`/shared adapter.
- [ ] Preserve every REST route/action and MCP action/schema expected by Cortex.
- [ ] Preserve scope/resource/auth behavior.
- [ ] Run surface parity and OAuth tests.

## Wave 7: Runtime, operations, and Cortex composition

- [ ] Create `cortex-runtime` with explicit builder/composition API.
- [ ] Move config/runtime state and maintenance/listener lifecycle ownership.
- [ ] Reuse Soma shared observability/HTTP/auth engines where contracts align.
- [ ] Create `cortex-ops` for setup/doctor/deploy/update mechanics that remain reusable.
- [ ] Create/finish `apps/cortex` as the canonical thin binary composition.
- [ ] Build `cortex --help`, HTTP server, stdio MCP, CLI, agent, and local-only operation modes from extracted crates.

## Wave 8: Cutover and de-duplication

- [ ] Run complete Cortex donor behavior/surface suite against composed Soma workspace crates.
- [ ] Run live-safe smoke tests that do not mutate homelab state.
- [ ] Remove obsolete duplicated donor modules.
- [ ] Prove no business logic remains duplicated between Cortex app and shared crates.
- [ ] Sweep all docs, examples, manifests, CI, release metadata, and dependency references.
- [ ] Re-run full Soma workspace gates.
- [ ] Record final architecture and dependency graph.

## Wave 9: Publication review

For each crate independently:

- [ ] API/feature stability review complete.
- [ ] README/rustdoc/metadata ready for crates.io.
- [ ] Real consumer or maintained consumer fixture exists.
- [ ] Semver owner/release component configured.
- [ ] License/security/dependency review complete.
- [ ] `publish = false` removed only for crates explicitly approved for publication.

## Per-crate definition of done

A crate cannot be marked extracted until all boxes below are true for that crate:

- [ ] Responsibility and non-goals documented.
- [ ] Donor source paths/commit recorded.
- [ ] Public API is narrow and storage/transport leakage reviewed.
- [ ] Workspace package/lint/architecture metadata follows Soma conventions.
- [ ] Explicit feature defaults defined.
- [ ] Existing donor tests moved or superseded with stronger tests.
- [ ] External-consumer test/fixture compiles only against public API.
- [ ] Rustdoc builds warning-free.
- [ ] Architecture check passes.
- [ ] Targeted clippy/tests pass.
- [ ] Integration/all-features gates pass for the lane.
- [ ] Code review findings resolved and recorded.
- [ ] Product parity remains green or an owned integration cutover is completed in the same merge wave.
