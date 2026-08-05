---
title: "Agent Runtime Implementation Package"
created: 2026-08-05
updated: 2026-08-05
doc_type: "implementation-index"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Implementation Package

This directory turns the specifications and contracts into an ordered, code-grounded delivery program.

- [PHASES-00-05.md](PHASES-00-05.md): contracts through compiled context.
- [PHASES-06-10.md](PHASES-06-10.md): materialization through disclosure.
- [PHASES-11-15.md](PHASES-11-15.md): lifecycle, synthesis, APM, and E2E.
- [FILE-PLAN.md](FILE-PLAN.md): exact proposed files by pull request.
- [CODE-SKELETONS.md](CODE-SKELETONS.md): code blueprint index.
- [CODE-SKELETONS-CORE.md](CODE-SKELETONS-CORE.md): config, ports, run, snippet, context code.
- [CODE-SKELETONS-RUNTIME.md](CODE-SKELETONS-RUNTIME.md): Incus, Codex, LABBY, disclosure, events.
- [CODE-SKELETONS-SYNTHESIS.md](CODE-SKELETONS-SYNTHESIS.md): Code Mode, Axon research, APM, bootstrap.
- [TEST-MATRIX.md](TEST-MATRIX.md): required verification by phase.

The code skeletons are not committed implementation. Their purpose is to pin ownership, call flow, reuse boundaries, error behavior, and acceptance tests before code mutation begins.
