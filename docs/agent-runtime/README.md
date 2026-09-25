---
title: "Soma Agent Runtime Documentation Package"
created: 2026-08-05
updated: 2026-08-05
doc_type: "documentation-package"
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

# Soma Agent Runtime Documentation Package

This package defines the post-context-v1 agent runtime built from Soma, Axon, Cortex, LABBY, Incus, Codex app-server support, and Agent Package Manager.

It is intentionally separate from <code>docs/unify/</code>. The context-v1 package explicitly deferred APM, worker agents, Incus provisioning, and orchestration. This package specifies those deferred capabilities without changing the authority or scope of context v1.

## Product statement

A Soma agent stack is a versioned declaration of:

- the agent runtime and prompt;
- installed APM primitives and their locked identities;
- the context universe and compilation rules;
- reusable Code Mode snippets and required skills;
- the LABBY upstream and tool exposure policy;
- the Incus runtime, mounts, resources, and lifecycle policy;
- progressive disclosure rules;
- Cortex lifecycle observation and retention;
- expected outputs, verification, and completion conditions.

Soma resolves the declaration, creates a bounded run, provisions the runtime, issues capabilities, compiles and discloses context, executes the agent, records its lifecycle, and emits a reproducible result manifest.

## Reading order

1. [START-HERE.md](START-HERE.md)
2. [OVERVIEW.md](OVERVIEW.md)
3. [ARCHITECTURE.md](ARCHITECTURE.md)
4. [BASELINES.md](BASELINES.md)
5. [CODE-MAP.md](CODE-MAP.md)
6. [specs/](specs/)
7. [contracts/](contracts/)
8. [types/](types/)
9. [models/](models/)
10. [schemas/](schemas/)
11. [IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md)
12. [PROGRESS.md](PROGRESS.md)
13. [VALIDATION-REPORT.md](VALIDATION-REPORT.md)
14. [MANIFEST.yaml](MANIFEST.yaml)

## Authority

- Current behavior is authoritative only where this package cites implemented code.
- New behavior is normative only after its contract is accepted and implemented.
- JSON Schemas in <code>schemas/</code> define the proposed serialized shapes.
- Markdown contracts explain semantics not expressible in JSON Schema.
- The implementation plan must be updated whenever the pinned code baselines change.
