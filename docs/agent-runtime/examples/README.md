---
title: "Agent Runtime Examples"
created: 2026-08-05
updated: 2026-08-05
doc_type: "examples"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: false
last_reviewed: "2026-08-05"
---

# Examples

This directory contains one coherent read-only incident-investigator fixture:

- <code>soma.context.yaml</code>: context universe and incident view;
- <code>read-only.loadout.yaml</code>: scoped LABBY capabilities;
- <code>soma.stack.yaml</code>: one-shot Codex investigator in Incus;
- <code>trace-service-failure.snippet.md</code>: reusable Code Mode investigation;
- <code>compiled-context.json</code>: immutable context result;
- <code>run-manifest.json</code>: completed run receipt;
- <code>synthesis-result.json</code>: evidence-backed structured answer.

The files use deterministic placeholder IDs and digests so they validate as fixtures. They do not represent a live incident.
