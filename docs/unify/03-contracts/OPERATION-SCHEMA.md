---
title: "Operation Schema Contract"
created: 2026-08-01
updated: 2026-08-01
status: normative
---

# Operation Schema Contract

## Purpose

This contract defines the machine-readable source of truth for operations extracted from Synapse and consumed by Soma products. It applies the pipeline-unification rule that one canonical model owns lifecycle and semantics while CLI, MCP, REST, OpenAPI, documentation, compatibility adapters, and tests are generated projections.

The bundle is validated by `schemas/operation-contract.schema.json`.

## Canonical bundle

The repository MUST maintain one versioned operation contract bundle containing:

- bundle schema and generation versions;
- the pinned donor baseline and fixture digest;
- canonical operation specifications;
- product-owned legacy bindings;
- parameter and result schema references;
- valid and invalid semantic fixtures;
- expected diagnostic codes;
- implementation and test provenance.

No surface may maintain an independently edited copy of operation metadata.

## Canonical operation specification

Each canonical operation MUST define:

- `name` and `contract_version`;
- summary and owning shared domain;
- target kind and target-resolution contract;
- access, risk, reversibility, retry, and idempotency classifications;
- planning, progress, cancellation, fanout, and verification support;
- parameter and result schema identifiers;
- required backend capabilities;
- expected evidence kinds;
- redaction profile;
- compatibility and deprecation status.

Canonical specifications MUST NOT contain Flux/Scout ownership, Synapse scopes, REST routing, CLI flags, MCP tool names, or product policy.

## Product compatibility bindings

Legacy bindings are a separate product-owned collection. A binding records product and surface, tool/action/subaction, aliases, product authorization scope, historical destructive flag, transport availability, canonical operation/version, translator IDs, and removal conditions.

A legacy binding may map multiple legacy spellings to one canonical operation. Two canonical operations MUST NOT claim the same binding identity.

## Parameter and result schemas

Typed Rust request and output models are authoritative. JSON Schema is generated from those types or from an equally strict definition checked against them.

Schemas MUST use Draft 2020-12, be closed by default, carry stable `$id` values, express alternatives and bounds, reject ambiguous targets and control characters, version breaking changes, and exclude credentials or raw authorization tokens.

Compatibility adapters may accept aliases, but normalization MUST produce exactly one canonical typed request before engine admission.

## Semantic fixtures

Every operation MUST provide a minimal valid request, a representative full request when optional behavior exists, each required alternative group, invalid unknown and conflicting fields, invalid target identity, deadline and budget failures, authorization failures for mutations, stale plan or topology cases, representative success and backend failure, and verification outcomes when supported.

Fixtures assert normalized requests, target resolution, plans, fingerprints, diagnostics, results, verification, events, and legacy projections. Snapshot-only testing is insufficient for security and state-machine behavior.

## State machine

The canonical lifecycle is:

`requested -> planned? -> authorized? -> started -> progressed* -> succeeded|failed|cancelled|partial -> verified?`

Surface-specific status names are projections. They MUST preserve canonical meaning and may not merge execution success with verification success.

## Diagnostic contract

Diagnostics use stable machine codes and structured fields. Human messages may improve without breaking compatibility.

Each diagnostic defines code, category, severity, retry classification, mutation-send uncertainty, correctable field paths, redaction behavior, and mappings for CLI exit codes, HTTP status, MCP error data, and operation events. Products MUST NOT infer retry safety from prose.

## Generation pipeline

The required order is:

1. compile and validate typed models;
2. generate canonical JSON Schemas;
3. generate the operation bundle;
4. join product legacy bindings;
5. generate MCP schemas and help;
6. generate OpenAPI components;
7. generate CLI reference and completion metadata;
8. generate documentation tables;
9. generate compatibility and differential fixtures;
10. fail if the working tree differs from generated output.

Generated files carry a banner naming the generator and source inputs. Hand edits are prohibited.

## Drift prevention

CI MUST fail when:

- a canonical operation lacks typed models or schema references;
- a binding targets a missing or incompatible operation version;
- parsed donor metadata differs from the pinned fixture;
- generated files are stale;
- surfaces disagree on fields or safety classification;
- a mutation lacks required authorization, idempotency, plan, or verification declarations;
- a donor path has no extraction-ledger disposition;
- an infrastructure operation bypasses the canonical executor.

## Versioning

Additive optional fields may retain the contract version only when defaults preserve behavior. New required fields, changed defaults, changed target meaning, narrowed authorization, result-shape changes, or changed mutation effects require a new operation contract version.

Legacy aliases are deprecated through bindings, never by renaming canonical operations in place. Bundles include a deterministic digest consumed by clients, docs, and compatibility tests.

## Pipeline-unification lessons adopted

This contract carries forward the strongest patterns from Axon's pipeline-unification work:

- one canonical lifecycle rather than per-surface lifecycles;
- one durable schema with generated projections;
- explicit cancellation, retry, recovery, and verification semantics;
- structured errors instead of message parsing;
- stable IDs and idempotent event delivery;
- generated documentation checked in CI;
- characterization and differential tests before cutover;
- executable divergence checks rather than architectural promises.

## Completion gate

The schema contract is complete when all 59 pinned Synapse operations have full semantic entries, the schema and generators are reproducible, every public surface is generated or validated against the bundle, and a deliberate mismatch causes CI to fail.
