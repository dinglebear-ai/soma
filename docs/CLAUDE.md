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
last_reviewed: "2026-06-27"
---

# Documentation Instructions

This directory contains stable guides, generated compatibility artifacts,
durable contracts/specs, external reference snapshots, and working records for
the soma project and the Rust MCP server family it governs.

Both humans and LLM agents operate this codebase. Write docs, contracts, specs, examples, and commands assuming both audiences. Prefer structured, runnable, and self-contained content. Avoid prose that only makes sense in context of a prior conversation.

---

## Documentation Layers

Use the right layer for the job:

- `docs/*.md` — Stable orientation, architecture narrative, cross-cutting
  guidance, and how-to guides. These are the map.
- `docs/contracts/` — Durable schemas, example payloads, and normative adapter
  contracts. Keep examples valid against the schema and code.
- `docs/generated/` — Machine-produced compatibility artifacts. Regenerate;
  do not hand-edit except to repair a generator.
- `docs/specs/` — Draft or handoff specs. Promote accepted requirements into
  stable guides after implementation.
- `docs/adr/` — Accepted architecture decisions. Add new ADRs for durable
  cross-cutting decisions.
- `docs/sessions/` and `docs/superpowers/plans/` — Historical work records.
  Useful evidence, not source of truth.
- `docs/references/` — Captured external specs and upstream repopacks. Treat as
  source evidence at the captured revision; refresh from upstream when stale.

---

## Files in This Directory

| File | Purpose | Update when |
|---|---|---|
| `QUICKSTART.md` | Five-minute getting-started guide | The startup sequence, CLI commands, or port changes |
| `AUTH.md` | Auth model: bearer tokens, OAuth, startup guard, gateway case | Auth behavior or env vars change |
| `PATTERNS.md` | Canonical patterns for the entire rmcp server family | The module structure, thin-shim rule, or family-wide conventions change |
| `CI.md` | Workflow purpose, path-aware gates, runner trust model, release gates | GitHub Actions, required checks, or runner routing changes |
| `LINUX-RUNNER.md` | TOOTIE Docker runner setup, isolation, cache, troubleshooting | Linux runner labels, volumes, compose path, cache, or security model changes |
| `WINDOWS-RUNNER.md` | STEAMY native Windows runner setup and artifact checks | Windows runner labels, sccache, artifacts, or native build flow changes |
| `XTASKS.md` | `cargo xtask` automation reference | xtask commands or generated-doc gates change |
| `MCP-REGISTRY-PUBLISH-GUIDE.md` | How to publish a derived server to the official MCP registry | The mcp-publisher CLI, registry schema, or CI publish workflow changes |
| `CLAUDE.md` (this file) | Instructions for agents and contributors navigating this directory | The directory structure or doc authority changes |

---

## References

`docs/references/` contains snapshots of MCP, Claude Code, registry, tooling,
and upstream repo references. Treat these as evidence for the captured revision,
not editable local docs.

- Prefer `docs/references/mcp/` before web search when implementing or verifying MCP protocol behavior.
- Prefer `docs/references/claude-code/` before web search when checking captured
  Claude Code plugin/skill behavior.
- If the captured reference is suspected stale or ambiguous for a fast-moving spec area (elicitation, extensions, registry preview), verify against the upstream source before changing behavior.
- When upstream marks material as `preview`, `draft`, `proposal`, `RFD`, or `SEP`, mirror that status in any derived docs.

Do not treat seed transcripts or conversation context as sufficient evidence for what the spec requires. If spec behavior matters, cite the reference file.

---

## Naming

- Product identifiers are Soma-first: the canonical binary is `soma`,
  crates use the `soma-*` prefix, and runtime env vars use the `SOMA_*` prefix.
- The pattern family is `rmcp-server`. Member servers include `labby`, `axon`, `cortex`, `gotify-rmcp`, `unifi-rmcp`, `apprise-rmcp`, `tailscale-rmcp`, `unraid-rmcp`, and Soma.
- Do not rewrite captured reference snapshots or upstream repopacks to match current naming. Those files preserve provenance.

---

## Soma Product Guidance

This repo now ships Soma as the product runtime. Keep end-user docs focused on
using and extending Soma through drop-in providers. Generated scaffolding,
cargo-generate, and historical docs may still discuss scaffolds as a
capability, but stable docs should not describe Soma itself as a placeholder.

- Do not add compatibility aliases for previous product names or env prefixes.
- Keep `PATTERNS.md` family guidance accurate for Soma-derived servers.
- The `PATTERNS.md` patterns are normative across all family members.
  Deviation requires an explicit decision recorded in that repo.

---

## Working Artifact Directories

Working artifacts inform but do not override stable docs in `docs/*.md`.

- `docs/sessions/` — saved session notes and handoff records.
- `docs/superpowers/plans/` — durable implementation plans from skill-driven work.
- `docs/specs/` — handoff specs and draft designs that may become stable docs.
- `docs/generated/` — committed generated artifacts used by CI/API compatibility.

If a working artifact contains an accepted requirement, promote it into the
appropriate stable guide, contract, or ADR.

---

## Style

- Short, direct sections with clear ownership.
- Examples should be runnable as written. Verify port numbers, command names, and flag names against the code before committing.
- Keep generated or historical material out of guides. If something belongs in a guide, distill it; don't paste.
- Do not move broad architecture into narrow docs only. Top-level docs should remain the map.
- Env var names are authoritative in `crates/soma/contracts/src/config.rs`. If a doc disagrees with the code, update the doc.
- Runner labels and trust boundaries must be documented in `docs/CI.md` plus
  the focused runner runbook. Do not leave runner behavior only in session notes.
