---
title: "Agent Runtime Code Skeletons"
created: 2026-08-05
updated: 2026-08-05
doc_type: "implementation-plan"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Code Skeletons

These skeletons show the intended seams using Soma's current architecture. They are deliberately close to compilable Rust, but an implementation PR must reconcile exact imports, feature gates, context-v1 crate names, generated DTOs, and current-main APIs before landing.

## Reading order

1. [CODE-SKELETONS-CORE.md](CODE-SKELETONS-CORE.md)
   - appdata paths;
   - application port bundle;
   - stack resolution;
   - durable run transitions;
   - snippet resolution;
   - context compilation.
2. [CODE-SKELETONS-RUNTIME.md](CODE-SKELETONS-RUNTIME.md)
   - Incus workload extensions;
   - Codex assistant adapter;
   - LABBY loadout adapter;
   - progressive disclosure;
   - lifecycle outbox.
3. [CODE-SKELETONS-SYNTHESIS.md](CODE-SKELETONS-SYNTHESIS.md)
   - run-scoped Code Mode host;
   - dependent Axon research;
   - structured synthesis;
   - APM process adapter;
   - bootstrap wiring.

## Rules

- Do not paste these into one giant source file.
- Preserve the repository's sibling-module convention and public docs requirements.
- Keep concrete construction in <code>apps/soma/src/bootstrap.rs</code>.
- Keep orchestration and policy in application modules.
- Keep external process, storage, Incus, LABBY, Axon, Cortex, and Codex details behind ports.
- Retain donor commit and path comments in transplanted implementations.
- Use generated schema-backed DTO tests rather than hand-maintained duplicate fixtures.
