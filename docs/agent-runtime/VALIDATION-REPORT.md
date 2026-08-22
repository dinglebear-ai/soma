---
title: "Soma Agent Runtime Documentation Validation Report"
created: 2026-08-05
updated: 2026-08-05
doc_type: "validation-report"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Validation Report

## Scope

This report validates the documentation package only. It does not claim the proposed agent runtime is implemented.

## Source baselines

| Product | Commit |
|---|---|
| Soma | <code>c604d0d503068a64d95d59fcd70e60d6fadf571b</code> |
| Axon | <code>488684fc90e0726f79efeda5e8e3e07d2cb8981f</code> |
| Cortex | <code>6afa01ad46594f9ad0e7bd519cdbc44b46664002</code> |
| LABBY | <code>59699f459cc4a68ef72c23200d74fa67d040c474</code> |
| APM | <code>dcbaf654cf6de26bb845927d383dd2e2ef9cb723</code> |

This report covers the complete proposed documentation snapshot on <code>docs/agent-runtime-package-20260805</code>. It does not claim that the described product runtime is implemented.

## Package-local validation

Command:

~~~bash
python3 scripts/check-agent-runtime-docs.py --write-manifest
~~~

Validated:

- Markdown frontmatter across the package;
- repository-local Markdown links;
- JSON syntax for all schemas;
- JSON Schema draft 2020-12 structure;
- local schema references without network resolution;
- YAML parsing;
- Markdown snippet frontmatter and JavaScript extraction;
- seven example instances against their schemas;
- checksummed package manifest freshness.

Result: **passed**.

## Schema-backed fixtures

| Fixture | Schema | Result |
|---|---|---|
| <code>examples/soma.stack.yaml</code> | <code>agent-stack.schema.json</code> | passed |
| <code>examples/soma.context.yaml</code> | <code>context-manifest.schema.json</code> | passed |
| <code>examples/read-only.loadout.yaml</code> | <code>labby-loadout.schema.json</code> | passed |
| <code>examples/trace-service-failure.snippet.md</code> | <code>snippet.schema.json</code> | passed |
| <code>examples/compiled-context.json</code> | <code>compiled-context.schema.json</code> | passed |
| <code>examples/run-manifest.json</code> | <code>agent-run.schema.json</code> | passed |
| <code>examples/synthesis-result.json</code> | <code>synthesis-result.schema.json</code> | passed |

## Repository-native validation

| Check | Result | Evidence |
|---|---|---|
| <code>git diff --check</code> | passed | no whitespace errors |
| <code>cargo xtask check-docs</code> | partial | generated docs are current and Python platform policy passed; the unrelated SDK import timing gate measured 1509.790 ms against a 500 ms budget |
| <code>cargo xtask check-stale-claims</code> | passed | stale claim check passed |
| <code>cargo xtask check-schema-docs --check</code> | passed | schema docs are current |
| <code>cargo xtask check-architecture</code> | passed | 40 workspace packages and 92 internal edges validated |

## Implementation status

- Documentation package: complete; the canonical package validator passes.
- Product runtime code: not started.
- Runtime migrations: not started.
- Public surfaces: not started.
- End-to-end Incus/LABBY/Axon/Cortex/Codex run: not started.

No document in this package should be interpreted as evidence that those implementation milestones have shipped.
