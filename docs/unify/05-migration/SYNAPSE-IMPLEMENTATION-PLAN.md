---
title: "Synapse to Soma Shared-Crate Implementation Plan"
created: 2026-08-01
updated: 2026-08-02
status: normative
---

# Synapse to Soma Shared-Crate Implementation Plan

## Objective

Move Synapse's reusable operations runtime into shared crates under Soma and expose one canonical operation contract to standalone Synapse, embedded Soma, and later orchestration surfaces.

The result is one implementation of operation semantics consumed by standalone Synapse, embedded Soma, remote Soma-to-Synapse adapters, and later Labby orchestration.

**Cutover decision, 2026-08-02:** Synapse has no external consumers that require historical result shapes. Canonical JSON is therefore the runtime boundary. Historical Flux and Scout names may remain as optional request aliases, but legacy result JSON and Markdown projection are deleted rather than preserved.

## Baselines

- Soma architecture base: `feat/product-family-architecture` at `6ce76c31`.
- Implementation worktree: `~/workspace/soma/.worktrees/operations-foundation`.
- Synapse donor reviewed: `dinglebear-ai/synapse@8f1bb2ef`.
- Donor surface: 59 operations.
- Protocol baseline: rmcp 3.1.0.
- Current foundation: `soma-ops` passes 49 unit tests plus a digest-bound 59-operation donor semantic compatibility test.

Before source movement, update `donors.lock.toml` to the full donor commit and regenerate a digest-bound semantic fixture.

## Non-negotiable invariants

1. Synapse remains independently shippable.
2. Shared crates contain no product scopes, environment reads, surface protocols, or UI policy.
3. Typed models cross shared boundaries; dynamic JSON stops at adapters.
4. One canonical operation catalog drives all projections.
5. Mutations require explicit authorization evidence and send-state reporting.
6. Execution success and verification are separate.
7. Fanout preserves every per-target outcome.
8. Unknown totals and uncertain mutation state are never fabricated away.
9. Canonical behavior is verified through schema-backed operation matrices and driver conformance; exact legacy presentation parity is not required.
10. Donor code is deleted only after standalone Synapse consumes the shared implementation.

## Target repository layout

`crates/shared/operations/ops` owns contracts and models.

`crates/shared/operations/fleet` owns topology, SSH, forwarding, transfer, fanout, deadlines, and cancellation propagation.

`crates/shared/operations/infra` owns typed Docker, container, Compose, host, filesystem, process, logs, ZFS, transfer, and Incus engines.

Synapse product crates own compatibility adapters and public surfaces. The exact crate split is introduced only when it lowers coupling; no empty crate confetti.

## Delivery method

Use a stacked PR train. Every PR has its own worktree, is based on the branch directly below it, carries one architectural responsibility, and is independently testable. Restack after lower PRs merge.

Every PR includes source disposition updates, generated-fixture updates, focused tests, architecture checks, and a rollback note.

## PR train and exit criteria

### PR 1: architecture and donor freeze

Base: `main`. Existing branch: `feat/product-family-architecture`.

Deliverables:

- multi-distribution and operations-plane ADRs;
- dependency-layer enforcement;
- donor lock for Soma, Synapse, Axon, and Cortex;
- generated 59-operation legacy fixture;
- code-disposition ledger;
- operation and event contracts.

Exit: architecture checks pass, every donor module is classified, and fixture regeneration is reproducible from the pinned commit.

### PR 2: operations foundation

Base: PR 1. Existing branch: `feat/operations-foundation`.

Deliverables:

- finish `soma-ops` public API review;
- operation models, schema contract, JSON Schema, and external-consumer fixture;
- cancellation and clock ports needed by real executors;
- stable diagnostic-code registry;
- semantic fixture loader and validator;
- contract bundle generator;
- schema and generated-file freshness checks;
- mutation policy invariants enforced in constructors and validation.

Required tests:

- default and all-feature unit tests;
- schema self-validation and negative fixtures;
- deterministic plan and bundle digests;
- authorization expiry, target, operation, and plan binding;
- cancellation and deadline behavior under fake time;
- event sequence and idempotent delivery;
- deliberate drift fixture that must fail.

Exit: a foreign Cargo workspace can define and validate operations without Soma or Synapse dependencies, and all 59 donor entries carry more than name mappings.

### PR 3: history-preserving Synapse product import

Base: PR 2.

Import the Synapse product into Soma without redesigning behavior in the import commit. Preserve file history where practical and record unavoidable moves.

Deliverables:

- standalone Synapse application, CLI, MCP, API, and web packages under product-owned paths;
- existing tests, fixtures, npm wrapper, plugin metadata, install/package workflows, and docs;
- product build and release targets in the Soma workspace;
- no shared-crate dependency yet beyond `soma-ops` types required for adapters.

Exit: imported standalone Synapse matches the donor binary, CLI help, MCP schemas, REST OpenAPI, and test suite at the locked commit.

Import execution record: the locked donor provenance and reviewed import-tree object are recorded in `donors.lock.toml` for the temporary `crates/synapse/import` nested workspace. The linear-history squash landing preserves that locked 386-file snapshot while keeping the imported `synapse` and `xtask` packages outside Soma's root Cargo workspace. Root Just recipes verify, build, test, and produce the locked release binary using the donor release path. The donor cannot be published directly with `cargo package` because its existing `lab-auth` Git dependency has no crates.io version; changing that dependency belongs to a later product-native packaging slice, not the no-redesign import. The next slices split it into `crates/synapse/*` and `apps/synapse`; the temporary boundary is then removed.

### PR 4: canonical catalog and compatibility adapters

Base: PR 3.

Split `src/actions/operations.rs` into canonical operation specifications and Synapse-owned legacy bindings.

Deliverables:

- one canonical catalog generated from typed definitions;
- `LegacyOperationBinding` registry for all 59 operations;
- typed parameter normalization for Flux and Scout;
- direct canonical result validation and JSON return;
- stable diagnostic mapping to CLI, REST, and MCP;
- generators for help, schema, OpenAPI, and documentation tables.

Exit: all current surfaces are derived from or validated against the registry, and changing one required field in a surface fixture makes CI fail.

Implementation record: `synapse-application` began as a compatibility-only crate over `soma-ops`. The canonical cutover extends it into the product runtime over `soma-fleet` and `soma-infra`. It still cross-validates 59 canonical specifications, 59 historical request bindings, parameter and result schemas, and 33 diagnostic mappings. Flux and Scout requests may normalize into canonical parameters, but canonical results now return directly as schema-validated JSON. The obsolete Markdown and legacy result projector has been removed. The imported donor workspace remains unchanged and unlinked.

### PR 5: fleet foundation

Base: PR 4.

Extract host topology, SSH, forwarding, transfer, and fanout into `soma-fleet`.

Deliverables:

- `HostRecord`, endpoint, label, capability, and topology-revision models;
- host repository, resolver, clock, executor, transfer, and event-sink traits;
- OpenSSH implementation with strict known-host policy;
- connection pool keyed by host identity and topology revision;
- forwarding and transfer lifecycle guards;
- bounded fanout scheduler with per-target deadlines, cancellation, admission, and partial results;
- mock and process-backed driver conformance suites.

Security tests include host-key mismatch, changed endpoint revision, stale pooled connection, argument injection, timeout before spawn, timeout after send, cancellation, transfer bounds, pool shutdown, and fanout overload.

Exit: Synapse remote tests pass through fleet interfaces and no infrastructure domain imports SSH details.

Implementation record: `soma-fleet` is now a native shared crate with validated topology identities, SHA-256 topology revisions, exact-revision connection pooling, bounded command and transfer contracts, process-backed conformance execution, strict OpenSSH native multiplexing, owner-only forwarding sockets, observable transfer guards, lifecycle events, and stable-order bounded fanout. OpenSSH post-spawn cancellation and timeout report `RemoteCommandDetached` because the remote process may still be running. Product configuration precedence, command allowlists, authorization, and infrastructure semantics remain outside the crate. Live host-key mismatch and remote smoke verification remain product-gated evidence; deterministic tests prove that only strict known-host plans can be constructed.

### PR 6: read-only infrastructure engines

Base: PR 5.

Extract read-only paths first in this order:

1. `docker.info`, `docker.df`, images, networks, volumes;
2. container list, inspect, logs, and stats;
3. Compose list, status, config, and logs;
4. host inspect and storage inspection;
5. filesystem stat, list, read, tail, and hash;
6. process list and inspect;
7. log reads and filters;
8. ZFS pool, dataset, and snapshot inspection.

For each slice:

- define dedicated request and output types;
- define the canonical operation spec;
- implement local and remote drivers behind the same trait;
- normalize donor behavior into canonical output;
- return the canonical result family directly;
- run canonical schema, security, and driver-conformance fixtures;
- add a live smoke test only after deterministic mock tests pass.

Exit: standalone Synapse delegates every migrated read to `soma-infra`, and embedded Soma can execute the same read through an operations port without importing Synapse.

Implementation record: the first `soma-infra` slice defines neutral read-only host, Docker, Compose, and filesystem contracts. Host and Compose reads execute through `soma-fleet` with discrete argv and bounded output. The optional Bollard adapter is local-only, bound to one host topology revision, and maps generated SDK models into stable neutral types. Linux filesystem stat, preview, and SHA-256 hashing reuse the donor's `openat2` confinement with explicit roots, `BENEATH`, `NO_SYMLINKS`, `NO_MAGICLINKS`, preview limits, and hash ceilings.

The read-expansion slice adds Docker disk usage, one-shot bounded container logs and statistics, Compose logs, typed process snapshots, validated syslog/journal/dmesg/auth reads, and structured ZFS pool, dataset, and snapshot tables. Command-backed reads preserve discrete argv, local filtering, cancellation, byte ceilings, allowlisted process sorts and ZFS types, journal option-smuggling defenses, and structured dmesg permission diagnostics.

The canonical-cutover slice completes remote Docker composition, descriptor-confined local/remote file, tree, find, and tail queries, remaining host-system reads, and container process tables. `SynapseReadRuntime` executes all 35 read operations through typed ports and validates every result against the checked-in canonical result schema. The legacy result projector is deleted. The catalog contains 21 mutation operations, all of which fail closed until an operation-specific mutation path is implemented.

### PR 7: mutation framework and infrastructure mutations

Base: PR 6.

Add mutation slices only after the read foundation is stable.

Required framework:

- plan-before-execute with deterministic fingerprint;
- opaque product authorization evidence;
- expected target/topology revision checks;
- idempotency keys where safe retry is possible;
- explicit cancellation boundaries;
- mutation-send state;
- postcondition verification;
- rollback or recovery guidance;
- operation events and protected artifacts.

Migration order:

1. reversible container restart/start/stop;
2. Compose pull/up/down/restart/recreate;
3. bounded filesystem mkdir/write/move/copy/remove;
4. structured command execution;
5. ZFS snapshot mutations;
6. host-level or privileged operations last.

Destructive and privileged operations require plans and cannot be exposed by embedded Soma until product policy explicitly enables them.

Exit: each mutation has plan, authorization, send-state, verification, cancellation, and failure-injection tests, including the case where the backend succeeded but verification or event delivery failed.

Implementation record: the mutation-foundation slice reuses `soma-ops` deterministic plans, authorization evidence, idempotency, send-state, retry, and verification contracts. `soma-infra` adds conservative mutation failures plus bounded postcondition coordinators. Synapse first implemented container start, stop, restart, pause, and resume, plus Compose up and restart.

The artifact-pull slice adds `docker.pull`, `container.pull`, and `compose.pull`. Plans bind exact image references, including the container-resolved image and every selected Compose service image. Bollard pull streams emit canonical `ProgressEvent` values; progress delivery failures remain bounded result metadata and never rewrite execution truth. Completion is verified through the Docker image store, and successful results carry OCI artifact references plus runtime-state evidence. Cancellation and timeout before send are `NotSent`; uncertainty after the stream begins remains `Unknown`; stream completion without verified image identity is a failed or inconclusive terminal result.

The verified-build slice adds `docker.build` and `compose.build`. Build contexts are admitted beneath explicit roots, traversed without following symlinks, bounded by file and byte ceilings, and hashed over relative paths, modes, sizes, and regular-file content. Plans bind the exact source-context SHA-256 and output tags. Execution repeats context fingerprinting before send, uses discrete Docker or Compose argv with bounded logs and phase progress, preserves retry-never semantics, and verifies every output tag through the local Docker image store. Successful results carry OCI artifact and source-context evidence.

The replacement slice adds `container.recreate` and `compose.recreate`. Container planning binds a driver-native SHA-256 over image, environment, command, entrypoint, labels, working directory, user, volumes, host configuration, and networks, plus the image-pull choice. Execution rechecks that digest before removal, preserves those fields, records stop/remove/create/start stages, and verifies the replacement is running under the captured name. Compose planning binds normalized configuration and service pre-state; execution uses discrete `compose up -d --force-recreate` argv and verifies the exact healthy service set. Successful results carry diff and runtime-state evidence. Fourteen of 21 mutations are implemented, and seven remain fail-closed. Compose down, image removal/prune, exec, host execution, and file transfer remain later slices.

### PR 8: standalone Synapse cutover

Base: PR 7.

Replace the legacy dispatcher internals with the canonical runtime over the shared engines.

Deliverables:

- Flux and Scout parsers normalize to typed canonical requests;
- canonical results return directly as schema-validated JSON;
- Synapse product policy issues authorization evidence;
- MCP elicitation and CLI confirmation remain product adapters;
- REST, MCP, and CLI surfaces consume canonical names and schemas; historical names may remain optional aliases;
- activity, status, readiness, and observability remain product-owned;
- old service implementations are removed after canonical coverage and driver conformance are proven.

Exit: Synapse's complete test suite, destructive smoke suite, MCPorter suite, OpenAPI drift check, CLI snapshots, npm wrapper tests, and release packaging pass with no donor engine path remaining.

### PR 9: Soma embedded operations

Base: PR 8.

Add a Soma product port over the same catalog and engines.

Initial exposure is read-only. Soma maps principals, workspaces, and product policy to operation contexts and authorization evidence. Results project into Soma CLI, API, MCP, provider, and web surfaces without importing Synapse compatibility types.

Exit: one read-only operation from every supported domain runs end to end in embedded Soma, emits canonical events, and matches standalone Synapse canonical output.

### PR 10: Incus operations

Base: PR 9.

Implement Incus server and instance inspection, lifecycle, file transfer, exec, image, profile, network, and storage operations over the existing neutral Incus client.

The Incus client remains protocol-level. Operation planning, authorization, progress, verification, and product policy stay above it.

Exit: local-socket Incus conformance and live container lifecycle tests pass without adding remote mTLS scope unless separately approved.

### PR 11: remote adapter and Labby parity

Base: PR 10.

Implement remote execution through standalone Synapse and verify Labby discovery and invocation.

Required proof:

- embedded and remote adapters consume the same contract bundle;
- catalog and schema digests match;
- canonical request/result fixtures match;
- operation IDs, correlation, deadlines, cancellation, and events survive the boundary;
- remote authorization cannot widen product-issued scope;
- a safe operation is invoked through Labby after gateway discovery;
- disconnected devices report the exact failed hop.

Exit: the same conformance suite passes for embedded, local-driver, SSH-driver, and remote-Synapse implementations.

### PR 12: release, cutover, and donor retirement

Base: PR 11.

Deliverables:

- standalone Synapse artifacts produced from the Soma monorepo;
- release version and changelog synchronization;
- installation and upgrade migration guide;
- compatibility window and deprecation policy;
- rollback artifact and documented rollback test;
- donor repository converted to generated mirror or archived landing page;
- branch protection preventing two-way manual development.

Exit: install, upgrade, rollback, and fresh deployment are proven from release artifacts; all 59 operations pass semantic parity; no donor implementation can drift.

## Verification matrix

### Static architecture

- shared crates have no product dependencies;
- shared crates contain no `SYNAPSE_*` or `SOMA_*` reads;
- RMCP, Axum, CLI, and web frameworks are forbidden below product adapters;
- every donor path has exactly one disposition;
- every operation path enters through the canonical executor;
- package and feature dependency graphs remain acyclic.

### Contract and schema

- JSON Schema validates under Draft 2020-12;
- positive and negative fixture suites pass;
- schema, catalog, help, OpenAPI, and docs are fresh;
- all 59 bindings resolve to exactly one canonical operation/version;
- duplicate names, aliases, or bindings fail;
- deterministic bundle and plan digests are stable across platforms.

### Canonical behavioral verification

For each migrated operation, execute the canonical runtime against fake drivers and controlled environments and verify:

- accepted and rejected requests;
- normalization and defaults;
- target resolution;
- backend calls and ordering;
- canonical outputs and diagnostics;
- progress and terminal events;
- cancellation, retry, and verification behavior.

Intentional differences from the donor require a documented canonical contract decision and fixture update. Presentation-only differences do not block cutover because there are no external Synapse consumers.

### Driver conformance

The same suite runs against mock, local, SSH, embedded Soma, and remote Synapse drivers where applicable. Driver-specific tests may add coverage but cannot replace the shared suite.

### Security and failure injection

Mandatory cases include command injection, path traversal, symlink races, host-key changes, stale connections, oversized output, transfer limits, expired authorization, wrong target binding, stale plan, cancellation at every boundary, timeout before and after mutation send, partial fanout, event-sink failure, verification failure, and backend success with lost response.

### Product and release

- complete standalone Synapse tests;
- Soma focused and full CI;
- CLI human and JSON snapshots;
- MCP schema and live client tests;
- REST OpenAPI and route tests;
- web smoke tests;
- npm/plugin packaging;
- release archive and checksum validation;
- install, upgrade, rollback, and clean-deploy tests.

## Observability and Cortex

Shared engines emit canonical operation events through an injected sink. They do not depend on Cortex storage. Embedded mode may use an in-process sink; standalone and remote modes use a bounded durable outbox when configured.

Event delivery failure after an external mutation is recorded separately and never rewrites execution truth. Cortex preserves original events and provenance before graph projection.

## Cutover and rollback

Each vertical slice has a product feature gate while canonical verification runs. Cutover requires complete canonical coverage, no direct donor runtime calls, and a rollback path that does not require data conversion.

The final cutover requires release artifacts from the monorepo, config compatibility, state and cache compatibility, preserved operation names, documented changed behavior, and a tested rollback release.

No two-way source synchronization is allowed after final cutover.

## Risk register

| Risk | Failure mode | Control |
|---|---|---|
| Name-only parity | 59 operations appear mapped while behavior drifts | schema-backed execution matrix plus driver conformance |
| Product leakage | shared crates acquire scopes, env vars, RMCP, or UI policy | architecture lint and forbidden-import scan |
| Dual runtime drift | donor and shared engines evolve independently | feature-gated cutover, then delete donor engine code |
| Oversized migration PRs | review cannot prove behavior | one vertical slice and one responsibility per PR |
| Mutation ambiguity | timeout hides whether a change happened | mutation-send state, idempotency, verification, recovery guidance |
| Stale topology | pooled SSH connection targets old endpoint | topology revision in target and pool key |
| Path escape | shared filesystem operations widen access | descriptor-confined paths and race-focused tests |
| SEP-2260 request association | elicitation fails after task spawning | keep product authorization/elicitation inside request scope and add regression tests |
| Generated drift | MCP, REST, CLI, and docs disagree | one generator pipeline plus clean-tree check |
| Release coupling | monorepo change breaks standalone Synapse release | independent build/release lanes and artifact-level smoke tests |
| Moving donor baseline | extraction chases upstream continuously | pinned commit; explicit, reviewed baseline refreshes only |

## Immediate execution checklist

The next implementation session should perform these tasks in order:

1. Update the Synapse donor lock and extraction docs from `b9255290` to the reviewed current baseline `8f1bb2ef`, using the full commit SHA.
2. Extend `generate-synapse-operation-fixture.py` to emit the complete legacy binding and source provenance, then compute a deterministic digest.
3. Add the operation-contract JSON Schema validator and positive/negative example bundles to xtask.
4. Extend the compatibility test from names to all currently knowable donor semantics: tool, action, subaction, access, destructive flag, transport, required fields, and alternatives.
5. Add canonical classification decisions for every one of the 59 entries, with explicit review of operations where the old destructive boolean is insufficient.
6. Add cancellation and clock ports plus a stable diagnostic-code registry to `soma-ops` before execution crates depend on it.
7. Add generated bundle, schema, docs, and clean-tree gates to `cargo xtask ci`.
8. Run public API, feature, rustdoc, package-content, architecture, and external-consumer reviews.
9. Regenerate the unify manifest and checksums.
10. Commit and open the operations-foundation PR before importing Synapse runtime source.

## Definition of done

The program is done when:

- standalone Synapse is released from Soma;
- all 59 operations have semantic parity or explicit versioned changes;
- Soma and Synapse consume the same operation catalog and engines;
- embedded, local, SSH, and remote adapters pass the same conformance suite;
- mutations prove authorization, send state, and verification;
- public surfaces are generated or mechanically drift-checked;
- the donor repository cannot accept divergent runtime development;
- install, upgrade, clean deployment, and rollback are independently verified.
