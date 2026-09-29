---
title: "Soma Agent Runtime"
created: 2026-08-05
updated: 2026-08-05
doc_type: "architecture"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "soma"
source_of_truth: true
upstream_refs:
  - "docs/unify/"
  - "docs/agent-runtime/"
last_reviewed: "2026-08-05"
---

# Soma Agent Runtime

Soma is evolving from a provider-backed MCP application into a declarative runtime for context-rich, isolated, and observable agents.

The runtime composes six existing systems rather than replacing them:

- **Soma** compiles context, applies policy, coordinates runs, and exposes one CLI, API, MCP, and web surface.
- **Axon** supplies refreshable knowledge, retrieval, durable research jobs, graph candidates, memory, and synthesis inputs.
- **Cortex** supplies continuous observations including commands, transcripts, logs, OTLP, Docker, heartbeats, inventory, and temporal evidence.
- **LABBY** supplies gateway discovery, scoped upstream exposure, Code Mode tool execution, and reusable snippets.
- **Incus** supplies isolated system-container or VM execution through Soma's existing local Unix-socket client.
- **APM** supplies portable, locked installation of prompts, skills, agents, hooks, plugins, and MCP dependencies. Soma remains the execution harness.

A declarative agent stack will bind an agent runtime, context manifest, prompts, skills, snippets, LABBY loadout, Incus profile, disclosure policy, observability policy, and expected outputs. Soma will compile that stack into a reproducible run, disclose context progressively, observe the full lifecycle, and preserve the evidence used for every conclusion and action.

The detailed specifications, contracts, models, schemas, examples, progress tracker, and implementation plan live in [docs/agent-runtime/](agent-runtime/README.md).
