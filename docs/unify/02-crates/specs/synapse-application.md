---
title: "synapse-application"
created: 2026-08-01
updated: 2026-08-02
status: implemented
---

# synapse-application

**Path:** `crates/synapse/application`
**Layer:** product application
**Package status:** private during extraction

## Purpose

`synapse-application` owns Synapse's checked-in operation catalog and canonical product runtime. It validates canonical requests, resolves hosts from `soma-fleet`, delegates all read operations to `soma-infra`, and validates canonical results before returning JSON.

There are no external Synapse consumers requiring historical response compatibility. Legacy Flux and Scout bindings remain only as optional request aliases and characterization data. The runtime does not rebuild legacy JSON or Markdown output.

The crate does not depend on `crates/synapse/import`, RMCP, Axum, Clap, a database, or environment configuration.

## Embedded contract set

At compile time the crate embeds and cross-validates:

- 59 canonical `OperationSpec` records;
- 59 historical `LegacyOperationBinding` records;
- 59 closed parameter schemas;
- 59 closed result schemas;
- 33 diagnostic surface mappings.

Classification digests, schema identities, required fields, alternative groups, binding keys, and diagnostic coverage are checked together when the catalog is constructed.

## Public boundary

- `SynapseCatalog`
- `SynapseReadPorts`
- `SynapseReadRuntime`
- `ExecutionError`
- `NormalizedOperationRequest`
- `OperationSchemaContract`
- `DiagnosticProjection`
- historical binding types for optional request aliases

## Canonical execution flow

1. Resolve a canonical operation and reject non-read operations before parameter processing.
2. Validate canonical parameters against the checked-in Draft 2020-12 schema.
3. Resolve the target from an immutable fleet topology snapshot.
4. Delegate to typed `soma-infra` ports with deadlines and cancellation.
5. Normalize the typed result into its canonical result family.
6. Validate the result against the checked-in canonical result schema.
7. Return canonical JSON directly.

Historical Flux and Scout requests may still normalize into this flow, but requested presentation fields do not control result rendering.

## Implemented reads

The runtime executes all 35 canonical read operations across product help, Docker, containers, host inspection, Compose, fleet topology, filesystem, processes, ZFS, and operating-system logs.

The remaining 24 mutation operations fail closed with `UnsupportedOperation` until the canonical mutation framework supplies planning, authorization evidence, send state, verification, and recovery semantics.

## Verification

- embedded counts: 59 operations, 59 bindings, 33 diagnostics;
- all 35 canonical read operations execute through mock ports and validate against their result schemas;
- mutation operations are rejected before parameter validation or driver access;
- generated historical Flux and Scout input schemas remain closed;
- no result projector or Markdown renderer remains;
- no imported donor dependency exists;
- strict Clippy, warning-free rustdoc, sibling tests, and architecture gates pass.
