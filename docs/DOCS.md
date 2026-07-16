---
title: "Documentation"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: true
last_reviewed: "2026-06-27"
---

# Documentation

This repo keeps documentation close to the automation it describes. Every file in `docs/` carries YAML frontmatter that describes its role, audience, and authority.

## Directory tree

```
docs/
  ├── README.md                       ← task-oriented docs index
  ├── DOCS.md                         ← documentation system and generation guide
  ├── CLAUDE.md                       ← agent/contributor instructions (source)
  ├── AGENTS.md                       ← symlink → CLAUDE.md (Codex CLI)
  ├── GEMINI.md                       ← symlink → CLAUDE.md (Gemini CLI)
  │
  ├── PATTERNS.md                     ← canonical pattern catalog (normative)
  ├── RMCP_README_GUIDE.md            ← root README shape for Rust MCP servers
  ├── MCP_SCHEMA.md                   ← MCP action/scope/schema contract
  ├── MCP-REGISTRY-PUBLISH-GUIDE.md  ← how to publish to the MCP registry
  │
  ├── adr/                            ← accepted architecture decision records
  │   ├── README.md
  │   └── 0001-stdio-first-plugin-adapter.md
  │
  ├── QUICKSTART.md                   ← 5-minute getting-started guide
  ├── PHILOSOPHY.md                   ← design principles
  ├── AGENTS-FIRST.md                 ← agent-first output and error design
  │
  ├── ARCHITECTURE.md                 ← module layout, layers, AppState
  ├── API.md                          ← HTTP endpoints, REST dispatch, errors
  ├── AUTH.md                         ← bearer tokens, OAuth, auth policy
  ├── CONFIG.md                       ← config.toml vs .env split, loading
  ├── ENV.md                          ← environment variable reference
  ├── OBSERVABILITY.md                ← /health, /status, logging, tracing
  │
  ├── DEPLOYMENT.md                   ← deployment modes overview
  ├── DOCKER.md                       ← Dockerfile, compose, entrypoint
  ├── SYSTEMD.md                      ← user-level systemd service
  │
  ├── PLUGINS.md                      ← Claude/Codex/Gemini plugin packaging
  ├── WEB.md                          ← embedded Next.js web UI
  │
  ├── CI.md                           ← GitHub workflows, nextest, taplo
  ├── LINUX-RUNNER.md                 ← TOOTIE self-hosted runner setup and trust boundary
  ├── WINDOWS-RUNNER.md               ← STEAMY native Windows runner setup and artifacts
  ├── PRE-COMMIT.md                   ← lefthook hooks, taplo, env guard
  ├── TESTING.md                      ← test strategy, sidecars, mcporter
  ├── MCPORTER.md                     ← live MCP integration testing
  ├── RUST.md                         ← Rust build setup: system tools, global config, per-repo rules
  ├── XTASKS.md                       ← cargo xtask commands
  ├── JUSTFILE.md                     ← just recipes reference
  ├── SCRIPTS.md                      ← scripts/ directory reference
  │
  ├── contracts/                      ← machine-readable JSON contracts
  │   ├── README.md
  │   ├── plugin-stdio-adapter.md
  │   ├── scaffold-intent.schema.json
  │   └── examples/
  │
  ├── generated/                      ← committed machine-produced artifacts
  │   ├── openapi.json
  │   ├── plugin-settings.md
  │   └── scripts-index.md
  │
  ├── specs/                          ← implementation specs and handoff docs
  │   ├── mcp-draft-2026-07-28-migration.md
  │   └── scaffold-intent-handoff.md
  │
  ├── sessions/                       ← saved session logs (transient)
  ├── superpowers/plans/              ← durable skill-driven plans
  └── references/                     ← captured upstream docs and repopacks
      ├── INDEX.md
      ├── CHANGES.md
      ├── claude-code/
      └── mcp/
```

## What goes where

| Location | What belongs there |
|---|---|
| `docs/*.md` | Stable orientation, architecture narrative, and how-to guides. The map, not the territory. |
| `README.md` | Canonical server surface and binary/transport profile policy for Soma users. |
| `docs/PATTERNS.md` | Normative patterns for the entire rmcp server family. Deviation requires an explicit recorded decision. |
| `docs/RMCP_README_GUIDE.md` | Reusable root README structure distilled from the current Rust MCP server family. |
| `docs/adr/` | Accepted architecture decisions. Use ADRs for cross-cutting choices that future adapters must preserve or explicitly supersede. Number new ADRs after the highest accepted record and add them to `docs/adr/README.md`. |
| `docs/contracts/` | Machine-readable JSON schemas and example payloads checked by CI scripts. Committed. |
| `docs/generated/` | Small artifacts produced by `just openapi`, `just schema-docs`, etc. Only commit when the artifact is part of CI/API compatibility checking. |
| `docs/specs/` | Implementation specs and handoff documents. Draft until promoted to a stable guide. |
| `docs/superpowers/plans/` | Durable implementation plans from skill-driven work. Keep only while still useful. |
| `docs/sessions/` | Saved session logs and handoff records written by `just save-session`. Transient. |
| `docs/references/` | Captured upstream docs and repopacks (MCP spec, registry, Claude Code docs, upstream repos). Refresh from source; do not rewrite snapshots by hand. |

Working artifacts (plans, reports, research, sessions) inform but do not override the stable docs in `docs/*.md`. Accepted requirements from working artifacts should be promoted into the appropriate stable guide.

## Frontmatter schema

Every `docs/*.md` file opens with YAML frontmatter:

```yaml
---
title: "Human-readable title"
doc_type: "guide"          # guide | contract | spec | adr | session | report
status: "active"           # active | draft | deprecated
owner: "soma"     # repo name or team
audience:
  - "contributors"
  - "agents"
scope: "soma"          # soma | service | family
source_of_truth: false     # true only when this file IS the canonical record
upstream_refs:             # optional: where authoritative info lives
  - "crates/soma/contracts/src/config.rs"
last_reviewed: "2026-05-15"
---
```

### Field meanings

| Field | Values | Purpose |
|---|---|---|
| `doc_type` | `guide`, `contract`, `spec`, `adr`, `session`, `report` | Classifies the file's role in the doc hierarchy. |
| `status` | `active`, `draft`, `deprecated` | `active` = current and maintained; `draft` = in progress; `deprecated` = superseded by another file. |
| `source_of_truth` | `true` / `false` | `true` only when this file IS the authoritative record. Most guides are `false` — they summarize the code or reference `PATTERNS.md`. When a doc disagrees with `source_of_truth: true` code, update the doc. |
| `upstream_refs` | file paths | Where to go when this doc and reality diverge. Code files beat docs. |
| `scope` | `soma`, `family`, `service` | `soma` = this repo only; `family` = normative across all rmcp servers; `service` = only relevant after Soma adaptation. |

### CLAUDE.md / AGENTS.md / GEMINI.md

`docs/CLAUDE.md` carries agent and contributor instructions for navigating this
directory. It is `source_of_truth: false` because the code, contracts, and
accepted ADRs are authoritative; the file explains structure and conventions,
not runtime behavior.

`docs/AGENTS.md` and `docs/GEMINI.md` are symlinks to `docs/CLAUDE.md` — they exist so Codex CLI and Gemini CLI find the same instructions Claude Code does. Their frontmatter is identical to `docs/CLAUDE.md` because they are the same file:

```yaml
---
title: "Documentation Instructions"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
upstream_refs:
  - "docs/references/mcp/"
last_reviewed: "2026-05-14"
---
```

After adding any new `CLAUDE.md` anywhere in the repo, regenerate the symlinks:

```bash
just symlink-docs
# or: cargo xtask symlink-docs
```

## Generated and checked docs

### Volatile docs

Regenerate and check volatile docs and metadata with:

```bash
cargo xtask generate-docs
cargo xtask check-docs
```

This updates `docs/ENV.md`, `.env.example`, `config.soma.toml`, plugin
manifests, `apps/web/lib/generated-actions.ts`,
`docs/generated/plugin-settings.md`, README/skill action tables, and
`docs/generated/scripts-index.md` from canonical Rust metadata.

### Schema docs

Regenerate and check MCP schema docs with:

```bash
just schema-docs
just schema-docs-check
```

The checker treats `crates/soma/contracts/src/actions.rs::ACTION_SPECS` as canonical. `docs/MCP_SCHEMA.md` and `docs/generated/openapi.json` must stay in sync with it.

### OpenAPI docs

Regenerate and check REST OpenAPI docs with:

```bash
just openapi
just openapi-check
```

The generator derives REST-capable actions from `ACTION_SPECS` and excludes MCP-only actions.

### Reference docs

`docs/references/` is populated by `scripts/refresh-docs.sh` and committed when
the captured external state is useful to preserve:

```bash
just refresh-docs              # full refresh (crawl + repomix)
just refresh-docs-dry          # dry run, no mutations
just refresh-docs-repomix      # skip crawl, repomix only
just refresh-docs-crawl        # skip repomix, crawl only
```

Run when starting work on MCP protocol, registry, Claude Code plugin/skill, or
other upstream-sensitive behavior; when an upstream releases a relevant new
version; or monthly to keep reference material current. Do not hand-edit the
captured snapshots. Distill accepted implications into stable guides, contracts,
or ADRs.
