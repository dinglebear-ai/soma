---
title: "Justfile"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
last_reviewed: "2026-06-18"
---

# Justfile

`Justfile` is the operator command surface for local development, CI parity, Docker, plugin packaging, and diagnostics. Run `just --list` for the complete current list.

## Core development recipes

| Recipe | Purpose |
|---|---|
| `just dev` | Run HTTP MCP server on loopback in no-auth dev mode (`SOMA_MCP_NO_AUTH=true`). |
| `just mcp` | Run stdio MCP transport (`soma mcp`). |
| `just greet` | Quick CLI smoke test. |
| `just doctor` | Pre-flight environment/connectivity checks (`soma doctor`). |
| `just build` / `just build-release` | Debug/release Rust builds. |
| `just build-local` / `just build-local-release` | Build only the lightweight `soma` CLI + stdio MCP binary. |
| `just build-server-release` | Build only the full `soma-server` API + Web + HTTP MCP binary. |
| `just build-web` | Build static Next.js web assets (`apps/web/out`). |
| `just web-check` | Run frontend lint, typecheck, tests, and static build. |
| `just build-full` | Build web assets then the full server release binary (CI/Docker use). |
| `just gen-token` | Generate a random bearer token (`openssl rand -hex 32`). |

## Quality gates

| Recipe | Purpose |
|---|---|
| `just verify` | `fmt-check` + `lint` + `check` + `test`. |
| `just soma-check` | Pattern/plugin/schema/Soma checks. |
| `just pre-release` | Full release-readiness gate (`cargo xtask pre-release-check`). |
| `just fmt` | Format Rust and TOML. |
| `just fmt-check` | Check formatting (CI). |
| `just lint` | `cargo clippy -- -D warnings`. |
| `just test` | `cargo nextest run`. |
| `just test-ci` | `cargo nextest run --profile ci`. |
| `just fmt-toml` | `taplo format`. |
| `just check-toml` | `taplo check` (CI). |

## Deployment recipes

| Recipe | Purpose |
|---|---|
| `just docker-build` | Build Docker image. |
| `just docker-up` / `just docker-down` | Start/stop compose stack. |
| `just docker-rebuild` | Rebuild image and recreate Docker service. |
| `just docker-logs` | Follow container logs. |
| `just runtime-current` | Detect stale running runtime (Docker or systemd). |
| `just auth-smoke` | Test bearer auth path against running server. |
| `just test-mcporter` | Run live MCP integration tests. |
| `just repair` | Rebuild and restart via systemd or Docker when available. |

## Plugin and xtask recipes

| Recipe | Purpose |
|---|---|
| `just build-plugin` | Validate plugin manifests, hooks, MCP config, and skills. |
| `just sync-bin` | Explicitly build and install the local Soma binary used by plugin launches. |
| `just validate-plugin` | Validate Claude/Codex/Gemini plugin manifests and skills. |
| `just fleet-alignment-check` | Check fleet plugin hooks, manifests, and required operator recipes. |
| `just dist` | `cargo xtask dist` — build and copy release artifacts. |
| `just ci` | `cargo xtask ci` — run all checks locally. |
| `just symlink-docs` | `cargo xtask symlink-docs` — sync `AGENTS.md`/`GEMINI.md` symlinks. |
| `just check-env` | `cargo xtask check-env` — validate required environment. |
| `just patterns-check` | `cargo xtask patterns` — check architecture contracts. |
| `just patterns-strict` | `cargo xtask patterns --strict` — fail on warnings. |
| `just patterns-json` | `cargo xtask patterns --json` — emit machine-readable findings. |

## Reference docs

```just
refresh-docs:           cargo xtask refresh-docs
refresh-docs-repomix:   cargo xtask refresh-docs --skip-crawl
refresh-docs-crawl:     cargo xtask refresh-docs --skip-repomix
refresh-docs-dry:       cargo xtask refresh-docs --dry-run
```

## Doctor output

```
$ soma doctor

soma-mcp v0.1.0 — environment check

  Config
  ──────────────────────────────────────────
  ✓ Config file:       ~/.soma/config.toml
  ✓ Data directory:    ~/.soma/ (writable)
  ✓ Binary in PATH:    /home/user/.local/bin/example

  Service credentials
  ──────────────────────────────────────────
  ✓ SOMA_API_URL:   https://example.internal/api (set)
  ✗ SOMA_API_KEY:   not set
    → Set SOMA_API_KEY in ~/.soma/.env

  Connectivity
  ──────────────────────────────────────────
  ✓ Upstream reachable: https://example.internal/api → 200 OK (42 ms)

  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  1 issue found. Fix it before running: soma-server serve
```

Exit code 0 = ready. Exit code 1 = issues found.

See `docs/PATTERNS.md` §48 for the full doctor command pattern.
