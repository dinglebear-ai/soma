---
title: "synapse-application"
created: 2026-08-01
updated: 2026-08-03
status: implemented
---

# synapse-application

**Path:** `crates/synapse/application`
**Layer:** product application
**Package status:** private during extraction

## Purpose

`synapse-application` owns Synapse's checked-in operation catalog and canonical product runtimes. It validates canonical requests, resolves immutable fleet targets, delegates typed work to `soma-infra`, and validates canonical results before returning them.

There are no external Synapse consumers requiring historical result compatibility. Flux and Scout bindings remain optional request aliases and characterization data only. The runtime does not rebuild legacy JSON or Markdown output.

The crate does not depend on `crates/synapse/import`, RMCP, Axum, Clap, a database, or environment configuration.

## Embedded contract set

At compile time the crate embeds and cross-validates:

- 59 canonical `OperationSpec` records;
- 59 historical `LegacyOperationBinding` records;
- 59 closed parameter schemas;
- 59 closed result schemas;
- 33 diagnostic surface mappings.

## Public boundary

- `SynapseCatalog`;
- `SynapseReadPorts` and `SynapseReadRuntime`;
- `SynapseMutationPorts` and `SynapseMutationRuntime`;
- `ExecutionError`;
- `NormalizedOperationRequest`;
- `OperationSchemaContract`;
- `DiagnosticProjection`;
- historical binding types for optional request aliases.

## Canonical read flow

1. Reject non-read operations before parameter processing.
2. Validate canonical parameters against the checked-in schema.
3. Resolve the target from an immutable fleet topology snapshot.
4. Delegate to typed `soma-infra` ports with deadlines and cancellation.
5. Normalize into the canonical result family.
6. Validate the result schema.
7. Return canonical JSON directly.

All 35 canonical read operations execute through this path.

## Canonical mutation flow

1. Validate the canonical mutation and parameters.
2. Resolve the exact host and topology revision.
3. Build a deterministic `OperationPlan` with target, change, step, verification strategy, and rollback guidance.
4. Require an exact plan fingerprint, operation identity, target binding, topology revision, authorization scope, expiry, and confirmation reference.
5. Require an idempotency key when the canonical operation contract declares idempotency.
6. Execute through a mutation-capable `soma-infra` port.
7. Preserve `NotSent`, `Sent`, or `Unknown` backend send state.
8. Verify the postcondition through a separate read operation.
9. Build and validate a canonical `OperationResult` with retry policy, verification, diagnostics, and recovery guidance.

## Implemented mutations

Twelve of the 21 canonical mutations are implemented:

- `container.start`, `container.stop`, `container.restart`, `container.pause`, and `container.resume`;
- `compose.up` and `compose.restart`;
- `docker.pull`, `container.pull`, and `compose.pull`;
- `docker.build` and `compose.build`.

Container lifecycle operations verify through `container.inspect`. Compose lifecycle operations verify through `compose.status`. Pull operations bind exact image references and verify IDs/tags/digests through `docker.images`. Build operations bind exact source-context SHA-256 values and output tags, re-fingerprint immediately before send, preserve bounded build logs and phase progress, and verify output identities through `docker.images`. OCI artifact, runtime-state, and source-context evidence are attached to successful artifact operations. Already-satisfied container states return a verified no-op without sending a mutation.

The remaining nine mutations fail closed with `UnsupportedOperation`.

## Verification

- all 35 canonical reads execute and validate their result schemas;
- all twelve implemented mutations plan, authorize, execute, and verify;
- stale topology, wrong target, expired authorization, missing confirmation, and missing idempotency fail before mutation send;
- cancellation before admission is `NotSent`;
- uncertain Docker failures remain `Unknown` and become failed terminal results;
- backend success plus failed postcondition verification is reported as failure;
- Compose command arguments are discrete and shell-free;
- container and Compose image-reference drift is rejected before pull send;
- progress delivery failure is retained separately and cannot rewrite mutation truth;
- successful pulls carry OCI artifact and runtime-state evidence;
- build plans bind exact source-context digests and reject context drift before send;
- Docker and Compose build commands use discrete argv, bounded logs, and retry-never execution;
- successful builds carry OCI artifact and source-context evidence;
- generated historical input schemas remain closed;
- no legacy result projector or imported donor dependency exists;
- strict Clippy, warning-free rustdoc, sibling tests, architecture, and pattern gates pass.
