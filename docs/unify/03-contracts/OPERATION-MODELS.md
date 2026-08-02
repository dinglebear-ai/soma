---
title: "Operation Domain Models"
created: 2026-08-01
updated: 2026-08-01
status: normative
---

# Operation Domain Models

## Rules

Models are product- and transport-neutral. Wire objects are versioned and closed by default. Compatibility aliases live in adapters. Unknown totals, partial success, mutation uncertainty, and inconclusive verification are explicit states. Secrets and unrestricted output are represented only by protected references.

## Identity

### OperationName

A stable lowercase dotted identifier such as `container.restart`.

- 2 to 8 segments;
- each segment starts with a lowercase ASCII letter;
- remaining characters are lowercase letters, digits, or underscores;
- maximum 128 bytes;
- meaning changes require a contract-version increase.

### IDs

- `OperationId`: UUIDv7 execution identity.
- `EventId`: UUIDv7 event identity, stable across delivery retries.
- `CorrelationId`: workflow or incident identity.
- `CausationId`: causing event or operation.
- `AuthorizationId`: opaque product-issued evidence identity.
- `SchemaId`: validated `schema.operations.<operation>.<parameters|result>.vN` identity bound to the operation name and positive schema version.
- `DiagnosticCode`: validated lowercase dotted machine code such as `target.not_found`; human messages are not stable identifiers.

### TargetRef

Fields: target kind, authority, canonical key, optional resource/topology revision, and a bounded parent chain. Display labels are metadata, never identity.

Target kinds include host, docker_daemon, container, compose_project, Incus server and instance, image, network, storage pool and volume, file, process, log source, ZFS pool/dataset/snapshot, and validated custom kinds.

## Catalog

### OperationSpec

Required fields:

- name and contract version;
- target kind;
- access, risk, and reversibility;
- parameter and result `SchemaId` values;
- stable diagnostic-code declarations;
- planning, progress, cancellation, verification, fanout, and idempotency support;
- retry classification;
- required backend capabilities;
- expected evidence kinds;
- redaction profile.

It never contains Flux/Scout names, REST exposure, product scopes, or UI labels.

### Classifications

- `AccessClass`: read or mutation.
- `RiskClass`: safe, disruptive, destructive, privileged.
- `Reversibility`: reversible, conditional, irreversible.
- `RetryClass`: never, safe, conditional. A safe mutation retry additionally requires explicit idempotency.
- `MutationSendState`: not_sent, may_have_been_sent, sent, confirmed_applied.

Reads cannot be destructive. Automatic mutation retry requires idempotency. Destructive and privileged operations require planning. Irreversible operations require explicit product approval policy.

## Requests

### OperationContext

Contains operation/correlation/causation IDs, actor and producer references, trace context, deadline, optional idempotency key, optional authorization evidence, expected target revision, output budgets, and bounded extension metadata.

### OperationRequest<P>

Contains context, typed parameters, resolved target, parameter digest, and optional plan fingerprint.

Validation order:

1. schema and bounds;
2. target resolution;
3. deadline and budgets;
4. catalog compatibility;
5. authorization binding;
6. target revision and plan fingerprint;
7. idempotency requirements;
8. backend capability availability.

## Authorization

### AuthorizationEvidence

Opaque evidence supplied by product policy:

- ID, issuer, and subject reference;
- issue and expiry times;
- exact operation and target scope;
- access/risk ceiling;
- optional plan fingerprint and confirmation reference;
- evidence format/version and integrity metadata.

Shared crates validate shape, expiry, and binding. They do not interpret users, OAuth claims, roles, or prompts.

## Planning

### OperationPlan

Contains operation and target identity, target/topology revision, normalized parameter digest, classifications, ordered steps, intended changes, prerequisites, conflicts, expected effects and blast radius, verification strategy, rollback/recovery guidance, expiry, and deterministic fingerprint.

The fingerprint covers every authorization-relevant field using canonical JSON and SHA-256.

### PlanStep

A one-based contiguous index, stable kind, bounded summary, target, effect class, optional estimated units, cancellation boundary, and compensation availability.

## Progress

### ProgressEvent

Contains operation ID, monotonic sequence, phase, optional step index, current value, optional total, unit, bounded summary, timestamp, optional per-target progress, and optional artifact reference.

Unknown totals remain null. Current never exceeds total. Progress is bounded, rate-limited, and never substitutes for terminal state.

## Results

### OperationResult<O>

Contains identity/name/target, status, timestamps, typed output or artifacts, diagnostics, per-target outcomes, mutation-send state, verification, evidence, redaction metadata, retry advice, and backend identity/version.

Statuses: succeeded, failed, cancelled, partial.

### Diagnostic

Contains stable code, severity, safe summary/detail, correctable field paths, retry classification, backend reference, and optional protected debug artifact.

### VerificationResult

Status is verified, failed, inconclusive, not_supported, or not_requested. It includes strategy, observations, expected state, evidence, and time. Transport success never implies verification.

## Events

### OperationEventEnvelope

Contains event ID/schema version/type/time, operation/correlation/causation identities, canonical name, actor/target/producer references, trace context, typed payload, and redaction metadata.

Payload variants: requested, planned, authorized, started, progressed, succeeded, failed, cancelled, verified. Terminal execution events are mandatory after started.

## Fleet

### HostRecord

Stable host ID, canonical name and aliases, platform/architecture, endpoints, labels, capabilities, connection-policy reference, topology revision, observation time, and disabled/draining state.

### FanoutResult<T>

Aggregate status, ordered per-target outcomes, admitted/started/completed/skipped/cancelled counts, shared deadline, concurrency limit, and partial-success diagnostics.

## Infrastructure request families

Every operation has a dedicated typed request. Generic `serde_json::Value` does not cross the engine boundary.

Families: Docker, container, Compose, host, filesystem, process, logs, ZFS, transfer, and Incus.

## Compatibility binding

Synapse owns `LegacyOperationBinding` with Flux/Scout tool, action/subaction, scope, destructive flag, transport availability, required/alternative legacy fields, canonical operation/version, and parameter/result translator IDs.

The binding is generated from the pinned donor and checked against the canonical catalog. It is not part of the shared execution model.
