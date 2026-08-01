---
title: "soma-ops"
created: 2026-07-31
updated: 2026-08-01
---

# soma-ops

**Path:** `crates/shared/operations/ops`
**Delivery phase:** Operations foundation
**Status:** Implemented; external consumer verified
**Publication:** Private until Soma and standalone Synapse both consume the contract in end-to-end slices.

## Purpose

`soma-ops` is the product-neutral contract crate for infrastructure operations. It defines how an operation is identified, targeted, classified, planned, authorized, reported, verified, and recorded without importing product policy or execution backends.

The crate is stewarded by the Synapse operations domain but remains independently usable by unrelated Rust applications.

## Responsibilities

- UUIDv7 operation, event, correlation, and authorization identities
- Validated dotted operation names
- Versioned operation parameter/result schema identities
- Stable validated diagnostic codes
- Typed target references and bounded parent relationships
- Access, risk, reversibility, retry, mutation-send, and verification classifications
- Operation catalog metadata and typed operation definitions
- Request contexts, deadlines, idempotency, and opaque authorization evidence
- Exact authorization binding to operation, target, expiry, and plan fingerprint
- Deterministic SHA-256 plan fingerprints
- Ordered plan steps, changes, prerequisites, conflicts, verification, and rollback guidance
- Bounded monotonic progress events
- Terminal results, diagnostics, artifacts, evidence, redaction, and independent verification
- Canonical lifecycle event envelopes suitable for Cortex ingestion
- Optional JSON Schema derivation

## Explicit exclusions

- Soma principals, workspaces, roles, or OAuth claims
- Synapse `synapse:read` and `synapse:write` scopes
- MCP elicitation or CLI confirmation prompts
- Docker, Compose, Incus, SSH, ZFS, filesystem, or log execution
- Host discovery or connection management
- Database, HTTP, runtime, or message-bus implementations
- Product environment variables and configuration defaults
- Cortex persistence or graph projection

## Public API

The initial public boundary includes:

- `OperationId`, `EventId`, `CorrelationId`, `AuthorizationId`
- `OperationName`, `SchemaId`, `DiagnosticCode`, `TargetKind`, `TargetRef`
- `OperationSpec`, `OperationDefinition`, `ParameterGroup`
- `OperationContext`, `OperationRequest`
- `AuthorizationScope`, `AuthorizationEvidence`
- `OperationPlan`, `PlanFingerprint`, `PlanStep`, `PlannedChange`
- `ProgressEvent`, `ProgressSink`
- `ExecutionMetadata`, `OperationResult`, `VerificationResult`
- `OperationEventEnvelope`, `OperationEventPayload`, `EventSink`

## Safety invariants

1. A canonical operation name is lowercase, dotted, and bounded.
2. Target, actor, producer, trace, diagnostic, and artifact references reject control characters and enforce size limits.
3. Read operations cannot claim destructive risk or mutation idempotency.
4. Safe automatic retry for a mutation requires explicit idempotency.
5. Destructive and privileged mutations require planning support.
6. Mutations require product-issued authorization evidence.
7. Authorization is bound exactly to operation, target, expiry, and optional plan fingerprint.
8. Idempotent mutations require an idempotency key.
9. Progress totals are never fabricated; unknown totals remain explicit.
10. Inline output is bounded; larger payloads become artifacts.
11. Failed and cancelled results require error diagnostics.
12. Execution success does not imply verification.
13. Failed or cancelled execution cannot be verified successful.
14. Lifecycle event identity remains stable across delivery retries.

## Compatibility fixture

`tests/synapse_compatibility.rs` consumes the pinned generated fixture at `docs/unify/03-contracts/examples/synapse-operations.json` and proves:

- all 59 donor operations are represented;
- legacy and canonical names and dispatch shapes are unique;
- every canonical name satisfies the neutral `OperationName` contract;
- Flux/Scout ownership, action/subaction, scope, destructive classification, transport, required fields, and alternative parameter groups are preserved;
- donor source path, ordered source lines, source hash, and per-macro hashes are valid;
- the exact donor commit and semantic-distribution counts are locked;
- a deterministic SHA-256 digest detects any semantic fixture edit.

`tests/synapse_canonical_classification.rs` consumes `synapse-canonical-operations.json`, deserializes every entry directly into `OperationSpec`, and proves complete canonical target, access, risk, reversibility, planning, progress, cancellation, verification, fanout, retry, idempotency, evidence, requirement, parameter-group, versioned schema-identity, and stable diagnostic-code coverage. Concrete parameter/result field schemas and surface-specific diagnostic projections remain later slices.

## Standalone consumer fixture

`tests/fixtures/external-consumer` is its own Cargo workspace and depends only on the public path package. It defines a foreign `host.inspect` operation, builds a typed request, and validates it without Soma or Synapse product crates.

## Dependencies

- `serde`
- `serde_json`
- `sha2`
- `thiserror`
- `uuid`
- optional `schemars`

The crate has no workspace path dependencies.

## Feature plan

- default: no optional dependencies
- `schema`: JSON Schema derivation through `schemars`

## Verification

- 49 unit tests
- one digest-bound 59-operation Synapse legacy semantic compatibility integration test
- one digest-bound 59-operation canonical `OperationSpec` classification integration test
- default-feature and all-feature test builds
- unrelated external Cargo consumer compile
- clippy with warnings denied
- rustdoc with warnings denied
- workspace architecture and sibling-test gates
- package-content and publication dry-run checks before publication

## Initial consumers

- standalone Synapse application and compatibility adapters
- `soma-fleet`
- `soma-infra`
- Soma embedded and remote operations adapters
- Cortex operation-event ingestion adapter

## Deferred work

- async sink adapter traits tied to a chosen runtime
- durable event outbox implementation
- cryptographic authorization signatures
- cancellation token implementation
- product-specific policy mapping
- backend-specific operation specifications

Those features remain outside the crate until a real consumer proves the boundary.
