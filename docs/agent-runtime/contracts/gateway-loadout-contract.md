---
title: "Gateway Loadout Contract"
created: 2026-08-05
updated: 2026-08-05
doc_type: "contract"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Gateway Loadout Contract

## Resolution input

A <code>LabbyLoadout</code> declares requested:

- mode: logical or physical;
- upstream allow and deny sets;
- tool allow and deny sets;
- snippet allow set;
- virtual-server policies;
- scopes and mutation classes;
- limits;
- credential references and subject policy;
- catalog pinning and refresh behavior;
- expiry.

## Resolution output

<code>LoadoutResolution</code> records:

- loadout digest;
- LABBY gateway identity and catalog generation;
- matched upstreams and tools;
- missing, denied, unhealthy, and quarantined entries;
- effective scopes and mutation class;
- applied limits;
- credential subjects by reference;
- policy and authorization decisions;
- logical token or physical gateway instance reference.

## Effective-catalog invariant

LABBY performs catalog filtering server-side. Soma and the agent runtime MUST NOT receive a global catalog and filter it only in the prompt or client.

## Logical mode

Logical mode uses a shared LABBY runtime with run-bound policy. The authorization mechanism MUST prevent reuse outside the bound run, agent, subject, expiry, and capability set.

## Physical mode

Physical mode creates or selects a dedicated gateway runtime with an immutable configuration receipt. It MUST still be registered and observed through Soma and Cortex.

## Catalog changes

A resolution pins a catalog generation by default. Refresh requires <code>loadout.refresh</code>, creates a new resolution generation, and reports additions, removals, and policy effects before activation.

## Tool calls

Every call carries run identity, resolved loadout generation, caller subject, tool scope, execution context, and trace context. LABBY's Code Mode host remains authoritative for dispatch, usage, OAuth, journaling, and tool errors.

## Errors

Required codes:

- <code>loadout_invalid</code>;
- <code>loadout_upstream_missing</code>;
- <code>loadout_tool_missing</code>;
- <code>loadout_capability_denied</code>;
- <code>loadout_gateway_unavailable</code>;
- <code>loadout_catalog_changed</code>;
- <code>loadout_credential_unavailable</code>;
- <code>loadout_physical_provision_failed</code>.
