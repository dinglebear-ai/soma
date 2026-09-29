---
title: "Agent Runtime Specifications"
created: 2026-08-05
updated: 2026-08-05
doc_type: "spec-index"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Specifications

These documents define required product behavior. They are broader than individual transport or serialized contracts.

| Specification | Scope |
|---|---|
| [agent-stack.md](agent-stack.md) | Compose-like workload declaration and resolution |
| [context-manifest.md](context-manifest.md) | Persistent context universe and policies |
| [compiled-context.md](compiled-context.md) | Immutable task-scoped evidence snapshot |
| [progressive-disclosure.md](progressive-disclosure.md) | Context, capability, evidence, and trust disclosure |
| [snippets.md](snippets.md) | Reusable Code Mode investigations with skills and permissions |
| [synthesis.md](synthesis.md) | Code Mode synthesis and dependent Axon research |
| [labby-loadouts.md](labby-loadouts.md) | Per-agent gateway and tool exposure |
| [incus-runtime.md](incus-runtime.md) | Isolated one-shot and resident execution |
| [assistant-runtime.md](assistant-runtime.md) | Codex app-server assistant integration |
| [lifecycle-observability.md](lifecycle-observability.md) | Full run and disclosure telemetry through Cortex |
| [apm-integration.md](apm-integration.md) | Package installation and lock integration |
| [context-filesystem.md](context-filesystem.md) | Filesystem and virtual-resource projections |

Every requirement uses **MUST**, **SHOULD**, or **MAY** in the RFC sense. A requirement is not implemented merely because it appears here; [PROGRESS.md](../PROGRESS.md) records delivery evidence.
