---
title: "Agent Stack Specification"
created: 2026-08-05
updated: 2026-08-05
doc_type: "spec"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Agent Stack Specification

## Purpose

An agent stack is the Compose-like deployment declaration for one or more cooperating agent services. It binds package content, context, Code Mode snippets, LABBY capabilities, Incus runtime configuration, disclosure, observability, outputs, and lifecycle policy.

The initial implementation MUST support one service. The schema permits multiple services so the contract does not require a breaking redesign when supervisors, researchers, reviewers, or monitors are added.

## Source file

The default file name is <code>soma.stack.yaml</code>. CLI flags MAY select another path.

~~~bash
soma stack validate
soma stack resolve
soma stack run
soma stack status RUN_ID
soma stack stop RUN_ID
soma stack inspect RUN_ID
~~~

The first implementation MAY expose these as actions under the existing compact Soma CLI/MCP surfaces rather than adding a large new top-level tool set.

## Required sections

### Metadata

A stack MUST include <code>apiVersion</code>, <code>kind</code>, <code>metadata.name</code>, and at least one service.

### Agent

The agent section MUST declare:

- runtime adapter, initially <code>codex-app-server</code>;
- package or prompt identity;
- execution mode: <code>one-shot</code> or <code>resident</code>;
- model and reasoning overrides only when supported by the adapter;
- acceptance criteria or expected output contract;
- approval policy.

### Package

A package MAY reference <code>apm.yml</code> and <code>apm.lock.yaml</code>. Soma MUST record the manifest and lock hashes in the resolved run. Soma MUST fail closed when a lockfile is required but missing or when the installed package drifts from the lock.

### Context

The stack MUST reference a context manifest or define an inline context block. The stack MAY select a named context view and pass task-specific parameters. Inline context policy can only narrow imported policy.

### Snippets and skills

A service MAY request snippets and skills. Snippet requirements MUST be resolved before provisioning. Missing required skills, tools, or context sources are validation failures, not runtime surprises.

### Gateway

A service MUST declare a LABBY loadout or explicitly choose an empty tool catalog. The loadout describes requested exposure, not final authorization.

### Runtime

The runtime section MUST declare:

- provider, initially <code>incus</code>;
- image or existing instance identity;
- project, profiles, devices, networks, and mounts;
- CPU, memory, disk, process, and timeout limits;
- environment and secret references;
- retention, snapshot, and cleanup policy.

The current Incus client only supports the local Unix socket. A stack that requests a remote Incus endpoint MUST be rejected until a secure remote transport is implemented.

### Disclosure

The service MUST declare a bootstrap disclosure set and MAY declare on-demand and restricted levels. The controller MUST record every disclosure decision.

### Observability

The service MUST declare whether commands, tool calls, transcript segments, file changes, process state, resource samples, OTLP, stdout, stderr, context requests, claims, and artifacts are captured. Security-sensitive raw payloads require explicit policy.

### Outputs

Outputs MUST be named and typed. At minimum every run emits:

- resolved stack;
- compiled context;
- run manifest;
- terminal status;
- structured synthesis result when synthesis is requested.

## Resolution

Resolution produces an immutable <code>ResolvedAgentStack</code>. It MUST include:

- canonical stack path and digest;
- imported manifests and digests;
- APM manifest and lock identities;
- resolved snippets and skills;
- effective capabilities;
- context view and parameters;
- concrete Incus image, project, profiles, mounts, and resource limits;
- runtime adapter configuration;
- observability and retention policy;
- policy decisions and warnings.

Resolution MUST occur before mutating Incus or starting a runtime.

## Dependency order

For each service, Soma resolves:

~~~text
package -> prompts/skills/agents/hooks/plugins/MCP
context manifest -> views -> source eligibility
snippets -> requirements -> declared outputs
LABBY loadout -> effective capabilities
runtime -> image/profiles/mounts/resources
observability -> collectors and retention
outputs -> artifact locations and quotas
~~~

## Lifecycle

The state machine is:

~~~text
created
-> resolving
-> resolved
-> provisioning
-> bootstrapping
-> running
-> verifying
-> finalizing
-> succeeded | failed | cancelled
~~~

Optional states include <code>awaiting_approval</code>, <code>stopping</code>, <code>snapshotting</code>, and <code>cleanup_failed</code>.

State transitions MUST be durable and idempotent. Axon's unified job state-machine patterns are the donor for lease, heartbeat, cancellation, recovery, and terminal publication.

## Security

- Requested tools MUST NOT imply authorization.
- Secret values MUST NOT be serialized into resolved manifests.
- Writable mounts MUST be explicit.
- Host-device passthrough MUST be denied by default.
- Mutation-capable actions MUST require explicit stack policy and caller authorization.
- A one-shot read-only investigator MUST be the default template.

## Validation

Validation MUST include JSON Schema, semantic cross-reference validation, path safety, policy intersection, environment/secret resolution, provider availability, Incus profile availability, LABBY upstream/tool existence, snippet requirements, and output quota checks.
