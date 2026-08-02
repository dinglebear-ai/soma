---
title: "Synapse Shared-Crate Extraction Specification"
created: 2026-08-01
updated: 2026-08-01
status: normative
---

# Synapse Shared-Crate Extraction Specification

## Product outcome

Soma becomes the source repository for reusable operations capabilities and for a complete independently shippable Synapse distribution. Synapse remains the operations-plane product. Soma may embed the neutral engines without inheriting Flux, Scout, Synapse authorization scopes, or standalone product policy.

There is one implementation of operation semantics, one canonical catalog, and one contract bundle. Product surfaces are adapters and projections.

## Scope

This program extracts:

- operation contracts, planning, authorization evidence, progress, results, verification, and events into `soma-ops`;
- host topology, SSH, forwarding, transfer, fanout, deadlines, cancellation propagation, and partial success into `soma-fleet`;
- Docker, container, Compose, host, filesystem, process, logs, ZFS, transfer, and Incus operations into `soma-infra`;
- the full standalone Synapse product into the Soma monorepo.

It does not merge Synapse product policy into Soma, make mutations generally available through Soma in the first release, replace the neutral Incus client, create generic crates without proven consumers, or preserve two independently editable runtime implementations.

## Authorities

- Product applications own principals, roles, workspaces, scopes, approvals, UI prompts, and policy defaults.
- `soma-ops` owns canonical operation semantics and lifecycle invariants.
- `soma-fleet` owns host identity, topology, and remote execution mechanics.
- `soma-infra` owns resource-domain behavior and drivers.
- Synapse owns Flux/Scout compatibility and standalone surfaces.
- Cortex owns persisted observation history and evidence projection, not execution truth.
- Low-level clients own protocol semantics only.

## Required interfaces

Shared crates expose typed Rust APIs for operation definitions and catalog lookup, request validation and target resolution, plan generation and fingerprinting, authorization-evidence validation, progress/cancellation/event sinks, execution and verification results, host repositories and resolvers, local and remote driver traits, and bounded fanout and transfer.

Dynamic JSON, MCP values, REST DTOs, and CLI argument maps terminate at product adapters.

## Compatibility

The locked Synapse baseline exposes 59 operations. Every operation must have one canonical operation/version or an explicit product-only classification, product-owned legacy bindings, typed canonical parameters and results, semantic fixtures, source/test provenance, and stable diagnostic mappings.

Flux and Scout names, MCP schemas, REST behavior, CLI help, JSON results, auth scope, plugin packaging, and npm wrapping remain compatible until a deliberate versioned release changes them.

## Execution contract

The lifecycle is requested, optionally planned, optionally authorized, started, progressed, terminal execution, and optional verification. Started execution always produces a terminal event. Execution success never implies verification.

Mutations require authorization evidence bound to operation and target, and to the plan fingerprint when planning applies. Safe automatic retry requires idempotency. Results report whether a mutation was not sent, may have been sent, was sent, or was confirmed applied.

Fanout returns ordered per-target outcomes and aggregate status. Cancellation and deadlines are propagated but never erase partial results.

## Security

Shared code rejects ambiguous targets, control characters, unbounded output, unsafe path traversal, command interpolation, stale topology, expired or mismatched authorization, stale plans, and prohibited retries.

Known-host verification is strict. Connection pools are invalidated by topology revision. Filesystem operations use descriptor-confined path policy. Product elicitation remains inside the MCP request association required by rmcp 3.1.0 and SEP-2260.

Secrets, tokens, private keys, unrestricted command output, and sensitive evidence are never placed inline in events or public results. They become protected artifacts with explicit retention and redaction metadata.

## Packaging and release

Synapse must build, test, package, install, upgrade, and roll back independently from the Soma monorepo. Release automation preserves the binary, npm wrapper, plugin surfaces, checksums, provenance, and container artifacts required by the product.

The original Synapse repository remains authoritative until monorepo release parity is proven. After cutover it becomes a generated mirror or archived landing repository. Two-way manual source synchronization is forbidden.

## Acceptance criteria

The extraction is accepted when:

1. all 59 operations pass semantic parity or carry approved versioned changes;
2. standalone Synapse consumes the shared crates for all reusable behavior;
3. Soma consumes the same catalog and engines without Synapse compatibility dependencies;
4. mock, local, SSH, embedded, and remote implementations pass common conformance tests;
5. public schemas and documentation are generated or drift-checked;
6. architecture checks prevent product leakage and bypass paths;
7. mutations prove planning, authorization, send state, cancellation, and verification;
8. install, upgrade, clean deployment, release, and rollback are verified from artifacts;
9. the donor runtime cannot drift after cutover.

## Normative references

- [Operation contract](../03-contracts/OPERATIONS-CONTRACT.md)
- [Operation event contract](../03-contracts/OPERATION-EVENT-CONTRACT.md)
- [Operation models](../03-contracts/OPERATION-MODELS.md)
- [Operation schema contract](../03-contracts/OPERATION-SCHEMA.md)
- [Machine-readable schema](../03-contracts/schemas/operation-contract.schema.json)
- [Code map](SYNAPSE-CODE-MAP.md)
- [Implementation plan](SYNAPSE-IMPLEMENTATION-PLAN.md)
