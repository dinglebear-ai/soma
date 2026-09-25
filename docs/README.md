---
title: "Documentation Index"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
upstream_refs:
  - "docs/DOCS.md"
  - "docs/CLAUDE.md"
last_reviewed: "2026-08-17"
---

# Documentation Index

Use this page as the map for `docs/`. The directory is intentionally mixed:
stable guides live next to generated contracts, external reference snapshots,
and historical session notes. Pick the right layer before editing.

## Start Here

| Need | Read |
|---|---|
| Run Soma quickly | `QUICKSTART.md` |
| Understand the architecture | `ARCHITECTURE.md`, then `PATTERNS.md` |
| Extract Cortex into reusable crates | `cortex-extraction/README.md`, then `cortex-extraction/PROGRESS.md` |
| Plan Python provider work | `specs/python-provider-platform.md` |
| Generate a new server | `SCAFFOLD.md`, `CARGO_GENERATE.md`, `contracts/scaffold-intent.schema.json` |
| Write or refresh an RMCP server README | `RMCP_README_GUIDE.md`, then `PATTERNS.md` |
| Add or change actions | `MCP_SCHEMA.md`, `API.md`, `AGENTS-FIRST.md`, `PATTERNS.md` |
| Operate CI or releases | `CI.md`, `LINUX-RUNNER.md`, `WINDOWS-RUNNER.md`, `XTASKS.md` |
| Package plugins | `PLUGINS.md`, `contracts/plugin-stdio-adapter.md` |
| Publish externally | `MCP-REGISTRY-PUBLISH-GUIDE.md`, `DOCKER.md`, `DEPLOYMENT.md` |

## Stable Guides

These are the human-facing docs to keep polished. They should summarize the
code, contracts, and workflows without becoming session logs.

| Area | Docs |
|---|---|
| Orientation | `QUICKSTART.md`, `PHILOSOPHY.md`, `AGENTS-FIRST.md`, `ARCHITECTURE.md`, `PATTERNS.md` |
| Runtime surfaces | `API.md`, `MCP_SCHEMA.md`, `WEB.md`, `PLUGINS.md` |
| Configuration and security | `CONFIG.md`, `ENV.md`, `AUTH.md`, `OBSERVABILITY.md` |
| Local development | `RUST.md`, `JUSTFILE.md`, `XTASKS.md`, `PRE-COMMIT.md`, `TESTING.md`, `MCPORTER.md`, `SCRIPTS.md` |
| Delivery | `CI.md`, `LINUX-RUNNER.md`, `WINDOWS-RUNNER.md`, `DOCKER.md`, `SYSTEMD.md`, `DEPLOYMENT.md` |
| Scaffold generation | `SCAFFOLD.md`, `CARGO_GENERATE.md`, `RMCP_README_GUIDE.md` |
| Documentation system | `DOCS.md`, `CLAUDE.md`, this index |

## Durable Records

These files are committed because they define behavior, compatibility, or
accepted design history.

| Directory | Purpose | Editing rule |
|---|---|---|
| `adr/` | Accepted architecture decision records | Add a new ADR for cross-cutting decisions; do not rewrite history casually. |
| `cortex-extraction/` | Durable Cortex extraction spec, contracts, inventory, tracker, verification, and review evidence | Keep the tracker synchronized with code and lane PRs until cutover is complete. |
| `contracts/` | JSON schemas, example payloads, and normative adapter contracts | Keep examples valid against the schema and code. |
| `specs/` | Draft or handoff specs for work that has not fully settled into guides | Promote accepted requirements into stable guides. |
| `generated/` | Machine-produced compatibility artifacts | Regenerate with the documented commands; avoid hand edits. |

## Working Records

| Directory | Purpose | Editing rule |
|---|---|---|
| `sessions/` | Saved session notes and handoff records | Historical evidence only. Distill useful decisions into stable docs. |
| `superpowers/plans/` | Durable implementation plans from skill-driven work | Keep if still useful; close the loop in stable docs when work lands. |
| `references/` | Captured external docs and upstream repopacks | Refresh from source; do not rewrite snapshots into local prose. |

## Generated And Checked Files

Use the generators instead of editing derived docs by hand:

```bash
cargo xtask generate-docs
cargo xtask check-docs
just schema-docs
just schema-docs-check
just openapi
just openapi-check
```

Key generated outputs include `docs/ENV.md`, `docs/MCP_SCHEMA.md`,
`docs/generated/openapi.json`, `docs/generated/plugin-settings.md`, and
`docs/generated/scripts-index.md`.

## Before Pushing Docs

For small prose-only edits:

```bash
cargo xtask check-docs
```

For docs that touch actions, config, scripts, releases, or workflows:

```bash
cargo xtask generate-docs
cargo xtask check-docs
cargo xtask check-stale-claims
```

For release-facing or Soma-shape changes:

```bash
cargo xtask pre-release-check
```

## Keep It Tidy

- Keep stable guides practical and current; move maintainer/process detail into
  the focused operations doc, not the README.
- Do not let `sessions/` become the source of truth. Promote the useful bit.
- Keep generated files generated.
- Update this index and `docs/DOCS.md` when adding, moving, or retiring docs.
