---
title: "LABBY Loadout Specification"
created: 2026-08-05
updated: 2026-08-05
doc_type: "spec"
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

# LABBY Loadout Specification

## Purpose

A LABBY loadout declares the gateway surface requested by an agent stack. It scopes discovery and execution to the upstreams and tools needed for one role or run.

The loadout does not replace LABBY's authoritative gateway configuration, catalog, virtual-server policy, OAuth, or runtime manager. Soma resolves a loadout against the live LABBY catalog and creates an effective exposure policy.

## Requested scope

A loadout MAY select:

- upstream namespaces;
- individual tools or action families;
- virtual MCP servers and their allowed tools, prompts, and resources;
- snippets;
- read, write, admin, or product-specific scopes;
- rate, concurrency, result-size, and call budgets;
- credential references and subject policy;
- network or device restrictions;
- mutation and approval classes.

Allow rules MUST be explicit. Deny rules MAY narrow broad allows. An empty loadout exposes no upstream tools.

## Effective capabilities

The final catalog is the intersection of:

~~~text
APM package requests
AND stack loadout
AND context-manifest policy
AND snippet requirements
AND LABBY live configuration
AND caller/agent authorization
AND runtime restrictions
~~~

The resolver MUST report missing, denied, stale, unhealthy, and quarantined upstreams or tools.

## Logical loadout

The default implementation is logical isolation on a shared LABBY gateway. Soma supplies a run identity and scope to a LABBY-hosted Code Mode host. Catalog listing and tool calls are filtered server-side.

The policy MUST bind to:

- run ID;
- agent/service ID;
- caller subject;
- issue and expiry time;
- allowed upstreams and tools;
- scopes and mutation class;
- limits;
- credential subject policy.

The agent must not receive an unscoped gateway credential.

## Physical gateway

A stack MAY request a dedicated LABBY process or sidecar when:

- the package is untrusted;
- process, filesystem, or network separation is required;
- incompatible upstream topology or versions are required;
- credentials must not enter the shared gateway process;
- tenant isolation exceeds logical policy guarantees.

Physical mode still uses the same loadout contract. Soma records the dedicated gateway instance and configuration digest.

## Donor behavior

Implementation MUST reuse or port:

- LABBY gateway configuration mutation;
- GatewayManager Code Mode host filtering and journaling;
- virtual-server surface and MCP policy;
- catalog change events and health views;
- snippet resolution;
- usage telemetry and call views;
- OAuth subject and upstream token behavior.

## Runtime changes

A LABBY catalog change during a run MUST produce one of:

- no effect because the run is pinned to a resolved catalog generation;
- controlled refresh accepted by policy;
- degraded run state when a required capability disappears.

The default is a pinned catalog generation with explicit refresh.

## Auditing

Every catalog listing and call MUST record run, agent, upstream, tool, scope, policy decision, duration, result class, error class, and artifact/UI links. Sensitive parameters follow existing Code Mode redaction policy.
