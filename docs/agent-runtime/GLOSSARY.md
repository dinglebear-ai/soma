---
title: "Agent Runtime Glossary"
created: 2026-08-05
updated: 2026-08-05
doc_type: "glossary"
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

# Glossary

**Agent package**: APM-managed collection of prompts, skills, agents, hooks, plugins, instructions, and MCP dependencies.

**Agent stack**: Soma declaration combining a package, context, snippets, gateway loadout, runtime, disclosure, observability, and lifecycle policy.

**Agent run**: One resolved and executed instance of an agent stack.

**Available context**: Context the run is authorized to query.

**Compiled context**: Immutable task-scoped selection of entities, records, evidence, graph paths, revisions, and time windows.

**Context manifest**: Versioned declaration of eligible context sources, graph roots, policies, budgets, prompts, skills, and saved views.

**Context pack**: Human- and tool-readable materialization of a compiled context. It is a view, not a canonical store.

**Disclosure**: A recorded act of revealing context, tools, skills, or evidence to an agent.

**Disclosure level**: Ordered depth ranging from bootstrap metadata to raw and cross-repository evidence.

**Effective capabilities**: Intersection of package requests, stack policy, context policy, LABBY loadout, runtime constraints, and caller authorization.

**Evidence bundle**: Bounded set of canonical records, citations, graph paths, summaries, and conflicts produced for a finding or question.

**LABBY loadout**: Requested upstream, tool, virtual-server, credential, rate, and mutation exposure for one agent or run.

**Logical gateway**: Shared LABBY gateway constrained by a signed or server-side run policy.

**Physical gateway**: Dedicated LABBY process or sidecar used when process or network isolation is required.

**Materialized context**: Context files or resources made available to the runtime, whether or not they were shown to the model.

**Progressive disclosure**: Policy and protocol for revealing deeper context only as required by the task.

**Resident assistant**: Long-lived agent runtime instance accepting multiple runs against a durable workspace.

**Run manifest**: Immutable record of resolved inputs, capabilities, runtime identity, disclosure history summary, outputs, and terminal state.

**Snippet**: Versioned Code Mode program with typed inputs, declared dependencies, permissions, output contract, and risk class.

**Synthesis**: Evidence-backed conclusion process that may calculate over context, launch dependent research, preserve conflicts, and emit structured claims.

**One-shot worker**: Ephemeral runtime instance created for one run and finalized according to retention policy.
