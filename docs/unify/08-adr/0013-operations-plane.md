---
title: "ADR 0013: Establish Synapse as the operations-plane product"
created: 2026-07-31
updated: 2026-07-31
---

# ADR 0013: Establish Synapse as the operations-plane product

**Status:** Accepted
**Date:** 2026-07-31

## Context

Soma already contains a neutral Incus REST client, while Synapse provides Docker, Compose, SSH, host inspection, logs, files, ZFS, fanout, and controlled mutation through 59 operations. Leaving Incus product behavior in Soma while Docker and host behavior remain in Synapse would divide one infrastructure domain across two products.

Cortex also ingests Docker, host, log, inventory, and command observations. That overlap is about subject matter rather than responsibility: Cortex preserves historical evidence, while Synapse inspects live state and performs controlled operations.

## Decision

Synapse is the standalone operations-plane product and the primary steward of neutral infrastructure operations crates.

The shared operations plane is divided into coarse boundaries:

- `soma-ops`: transport-neutral operation identity, targets, safety classification, plans, progress, verification, approvals, and events;
- `soma-fleet`: host topology, OpenSSH connectivity, connection pooling, forwarding, transfer, fanout, deadlines, and partial-success behavior;
- `soma-infra`: Docker, Compose, Incus, host, files, processes, logs, and ZFS operational semantics;
- the existing neutral `incus-client`: Incus protocol transport and resource models.

Synapse owns product-specific configuration, Flux and Scout compatibility, standalone authorization, confirmation, CLI, MCP, HTTP, web, setup, doctor, and release behavior.

Soma consumes the operations plane through a Soma-owned application port. It may use an embedded adapter over the neutral shared engines or a remote adapter through Labby to a standalone Synapse deployment.

Cortex consumes operation lifecycle events as observations. It does not execute operations. Synapse does not write directly to Cortex's database or graph.

## Incus boundary

The neutral Incus client remains under `crates/shared` and does not become product-coupled.

Synapse owns generic Incus operations such as instance, image, network, project, storage, and lifecycle behavior. Soma continues to own Soma-specific desired state such as appliance naming, image selection, setup, backup, upgrade, migration, and rollback policy.

## Safety lifecycle

Mutations follow a common lifecycle:

```text
request -> resolve target -> plan -> authorize -> execute -> progress -> verify -> emit outcome
```

Shared crates model authorization evidence but do not interpret Soma principals, Synapse scopes, OAuth claims, or interactive prompts. Product adapters perform those mappings.

## Consequences

- Docker and Incus become sibling operational backends.
- Flux and Scout remain standalone Synapse compatibility surfaces, not shared domain names.
- Soma's deployment policy becomes a consumer of generic operations instead of a second infrastructure engine.
- Cortex receives a complete before-and-after operational evidence trail without gaining mutation authority.
- Remote and embedded operations implementations must pass the same contract suite.

## Rejected alternatives

- Keep Incus operations inside Soma while Synapse owns Docker.
- Move the neutral Incus client into Synapse product crates.
- Fold Synapse execution into Cortex ingestion.
- Make Labby's gateway core understand Docker, Incus, ZFS, or filesystem business semantics.
- Define a lowest-common-denominator container API that hides Docker and Incus differences.

## Revisit when

Revisit crate granularity only after a subdomain has an independent consumer, a stable public contract, or a materially separate dependency and release cadence.
