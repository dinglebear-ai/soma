---
title: "Start Here: Soma Agent Runtime"
created: 2026-08-05
updated: 2026-08-05
doc_type: "guide"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Start Here

## The plan in one sentence

Extend Soma's existing context, provider, Code Mode, gateway, observability, Incus, and Codex foundations into a Compose-like agent runtime where manifests declare what an agent knows, what it can do, where it runs, how context is progressively disclosed, and how its complete lifecycle is observed.

## Non-negotiable boundaries

1. **Do not merge Axon and Cortex ingestion lifecycles.** They converge through context and evidence projections.
2. **Do not fork LABBY Code Mode.** Soma already contains a shared Code Mode crate; port missing snippet-store and scoped-gateway behavior into shared/product boundaries.
3. **Do not make APM the runtime.** APM installs and locks agent primitives. Soma governs execution.
4. **Do not give every agent the global LABBY catalog.** A run receives the intersection of package requests, manifest policy, loadout policy, and caller authorization.
5. **Do not mount the complete context into the prompt.** Available, mounted, disclosed, and cited context are distinct states.
6. **Do not treat transcript prose as verified truth.** Claims must retain evidence class, source, time, and confidence.
7. **Do not create a second graph.** A context pack is a bounded materialization of the shared evidence graph.
8. **Do not claim remote Incus support.** The current Incus client uses the local Unix socket only.

## First deliverable

The first vertical slice is a read-only, one-shot Codex investigator:

~~~text
soma stack run examples/soma.stack.yaml
  -> validate manifests and lock inputs
  -> compile repository and incident context
  -> create an Incus container locally
  -> mount repository and selected docs read-only
  -> issue a read-only LABBY loadout
  -> start Codex app-server through the existing client
  -> disclose the bootstrap context capsule
  -> run one investigation snippet through Code Mode
  -> record Cortex lifecycle events and artifacts
  -> emit compiled-context.json, run-manifest.json, and synthesis-result.json
  -> stop and retain or delete the container according to policy
~~~

No autonomous mutation is required for the first slice.

## Document roles

- **Specs** describe required product behavior.
- **Contracts** define stable boundaries and invariants.
- **Types** provide proposed Rust-facing DTOs and enums.
- **Models** explain aggregate ownership and lifecycle.
- **Schemas** validate serialized YAML and JSON.
- **Examples** demonstrate complete valid configurations.
- **Implementation plan** maps every phase to current code and tests.
- **Progress tracker** records delivery state and evidence.
