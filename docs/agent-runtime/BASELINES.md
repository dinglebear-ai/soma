---
title: "Agent Runtime Source Baselines"
created: 2026-08-05
updated: 2026-08-05
doc_type: "baseline"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Source Baselines

The implementation plan and contracts in this package were prepared from the following exact source snapshots.

| Product | Repository | Ref used | Commit | Role |
|---|---|---|---|---|
| Soma | <code>dinglebear-ai/soma</code> | <code>origin/main</code> | <code>c604d0d503068a64d95d59fcd70e60d6fadf571b</code> | Composition root, Code Mode, provider runtime, gateway, Incus client, Codex client, surfaces |
| Axon | <code>dinglebear-ai/axon</code> | <code>origin/main</code> | <code>488684fc90e0726f79efeda5e8e3e07d2cb8981f</code> | Knowledge, retrieval, jobs, graph candidates, memory, synthesis, LLM runtimes |
| Cortex | <code>dinglebear-ai/cortex</code> | <code>origin/main</code> | <code>6afa01ad46594f9ad0e7bd519cdbc44b46664002</code> | Logs, sessions, commands, OTLP, Docker, heartbeats, inventory, graph, incidents |
| LABBY | <code>dinglebear-ai/labby</code> | <code>origin/main</code> | <code>59699f459cc4a68ef72c23200d74fa67d040c474</code> | Gateway catalog, scoped Code Mode host, snippet store, virtual-server policies |
| APM | <code>microsoft/apm</code> | detached commit | <code>dcbaf654cf6de26bb845927d383dd2e2ef9cb723</code> | Portable agent primitives, manifests, lockfiles, policy, install lifecycle |

## Baseline policy

- Every implementation pull request must cite the full baseline commit for every donor file it uses.
- Donor code is behavioral evidence, not a runtime dependency unless the workspace already contains the crate.
- Prefer moving reusable mechanisms into <code>crates/shared</code> and keeping Soma policy in <code>crates/soma</code>.
- Re-audit this package before implementation if any baseline changes.
- Update schemas and examples in the same change as contract-breaking code.
- Do not silently substitute checked-out feature branches for the pinned upstream main commits.

## Known baseline constraints

- Soma's Incus client supports local Unix-socket transport only.
- Soma's shared Code Mode crate has host-resolved snippet types but no installed filesystem snippet store.
- LABBY has the more complete snippet store, promotion flow, and virtual-server policy implementation.
- Soma's current context-v1 package explicitly excludes APM and agent worker deployment.
- APM's package plane and Soma's execution plane are separate by design.
