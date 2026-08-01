---
title: "Operations Contract"
created: 2026-07-31
updated: 2026-07-31
---

# Operations Contract

This contract defines the product-neutral boundary used by standalone Synapse, embedded Soma operations, and remote Soma-to-Synapse operation adapters.

## Authority

- The operations engine is authoritative for live operation execution and returned runtime evidence.
- Product applications are authoritative for identity, authorization, approvals, workspace policy, and user-facing workflow state.
- Cortex is authoritative for persisted observation history and evidence-graph projection after operation events are ingested.
- Protocol clients such as the Incus client are authoritative only for transport and protocol semantics.

## Stable identities

Every request MUST carry or receive:

- `operation_id`: globally unique execution identity;
- `operation_name`: stable dotted capability name;
- `target`: typed target identity;
- `correlation_id`: workflow or incident correlation identity;
- optional `causation_id`;
- optional `idempotency_key` for mutations;
- an absolute deadline or explicit unbounded policy;
- a trace context safe to propagate across process boundaries.

Product-facing aliases such as Flux and Scout action names are adapters to stable operation names and are not canonical shared identifiers.

## Operation catalog

Each operation specification MUST declare:

- stable name and schema version;
- target kind;
- access class: `read` or `mutation`;
- risk class: `safe`, `disruptive`, `destructive`, or `privileged`;
- reversibility: `reversible`, `conditional`, or `irreversible`;
- required and alternative parameter groups;
- support for planning, progress, cancellation, and verification;
- retry classification;
- expected evidence outputs;
- implementation capability requirements.

Shared operation specifications MUST NOT contain product scopes such as `synapse:read` or `synapse:write`.

## Requests

Shared engines accept typed requests. Dynamic `serde_json::Value` action dispatch is permitted only at compatibility and transport adapters.

A request MUST reject unknown fields when the operation contract marks the shape closed, duplicate or conflicting options, ambiguous targets, control characters, invalid path or identifier encodings, deadlines outside configured bounds, and mutations without valid authorization evidence.

## Planning

A mutation MUST support a pre-execution plan when the backend can determine its effect. A plan SHOULD include:

- resolved target and topology revision;
- intended changes and affected resources;
- risk and reversibility;
- prerequisites and conflicts;
- expected operation steps;
- verification strategy;
- rollback or recovery guidance;
- a plan fingerprint bound to authorization evidence.

Execution MUST fail when a plan fingerprint is required and the target or topology has changed materially since authorization.

## Authorization evidence

The shared engine receives opaque authorization evidence containing authorization identity, approved operation and target scope, plan fingerprint when applicable, expiry, issuer, and an optional human-confirmation reference.

Shared crates validate evidence structure and binding but do not interpret product users, roles, OAuth claims, or UI prompts.

## Execution

Execution MUST:

- enforce deadline and cancellation semantics;
- bound request admission, fanout, output, traversal, and transfer size;
- preserve partial successes and per-target failures;
- avoid shell interpolation for structured command execution;
- prevent stale connection reuse after topology changes;
- emit progress and terminal events;
- classify whether a failed mutation may have been sent;
- return structured, correctable diagnostics.

## Results

A terminal result MUST include operation identity and name, resolved target identity, terminal status, timestamps, structured output or artifact references, per-target results for fanout, diagnostics, mutation-send state, verification status, evidence suitable for Cortex ingestion, and redaction metadata.

Large payloads MUST become bounded artifacts rather than unbounded inline responses.

## Verification

Mutations SHOULD define an explicit verification query. Verification is distinct from successful command or API completion.

Verification results are one of:

- `verified`;
- `failed`;
- `inconclusive`;
- `not_supported`;
- `not_requested`.

A product MUST NOT describe a mutation as verified merely because its transport call returned success.

## Compatibility

Standalone Synapse preserves its 59-operation Flux and Scout surface through adapters. Contract tests MUST prove that every legacy action maps to one canonical operation or an explicitly documented product-only action.

Embedded and remote implementations MUST pass the same request, safety, result, and event fixtures.
