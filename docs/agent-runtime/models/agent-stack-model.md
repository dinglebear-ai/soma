---
title: "Agent Stack Model"
created: 2026-08-05
updated: 2026-08-05
doc_type: "model"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Agent Stack Model

## Aggregate

<code>AgentStack</code> is desired state. <code>ResolvedAgentStack</code> is immutable execution input. The unresolved source file is never executed directly.

~~~text
AgentStack
  metadata
  services
    agent
    package
    context
    snippets + skills
    gateway
    runtime
    disclosure
    observability
    outputs
    lifecycle

ResolvedAgentStack
  source identity and digest
  resolved package inventory
  resolved context manifest/view
  resolved snippets and skills
  effective capabilities
  concrete runtime configuration
  output contracts
  policy decisions and warnings
~~~

## Consistency boundary

Resolution is atomic from the application's perspective. If any required package, snippet, skill, context view, LABBY capability, Incus resource, mount, secret reference, or output schema cannot be resolved, no resolved stack is published and no runtime is provisioned.

## Versioning

- Source stack schema version controls input parsing.
- Resolved-stack schema version controls persisted run compatibility.
- A source stack change creates a new digest and resolution.
- A running service is pinned to one resolved stack generation.

## Imports

Imports are resolved before service validation. Imported policy may only be narrowed. Cycles and ambiguous relative paths are errors.

## Multi-service future

Service dependencies may later add <code>dependsOn</code>, health conditions, shared context, shared artifacts, and explicit output bindings. V1 should parse multiple services but may reject stacks with more than one executable service using a stable <code>multi_service_not_supported</code> error.

## Separation from APM

APM packages primitives. AgentStack composes those primitives with context, capabilities, runtime, and observability. A stack may use no APM package, but a package cannot replace the stack.
