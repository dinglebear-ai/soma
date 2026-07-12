---
title: "ADR 0001: Stdio-first plugin adapter"
doc_type: "adr"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: true
last_reviewed: "2026-05-26"
---

# ADR 0001: Stdio-first plugin adapter

## Status

Accepted, 2026-05-26.

## Context

Soma now supports two binary profiles:

| Binary | Profile | Required surfaces |
|---|---|---|
| `example` | Lightweight local adapter | CLI + stdio MCP |
| `soma-server` | Full platform server | REST API + Web + Streamable HTTP MCP + health/auth |

Upstream-client MCP servers should be cheap to install locally and should not
run a local REST/Web mirror only because the upstream service has an HTTP API.
Application/platform servers still need a full server binary for Docker,
systemd, gateway, web, API, and remote MCP use.

Plugin installs sit between those profiles. They should launch a local
child-process MCP adapter, but for platform servers that adapter should call
the deployed platform API instead of starting or depending on a local HTTP MCP
server.

## Decision

- Plugin installs default to stdio MCP through the installed local binary.
- Claude Code and Codex use `plugins/soma/.mcp.json` with:
  - `"type": "stdio"`
  - `"command": "soma"`
  - `"args": ["mcp"]`
- Gemini uses the equivalent extension-local command:
  - `"command": "soma"`
  - `"args": ["mcp"]`
- Plugin settings inject `SOMA_API_URL` and `SOMA_API_KEY` into the
  stdio child process.
- Empty `SOMA_API_URL` means offline stub mode for local smoke tests and
  scaffolded examples.
- Non-empty `SOMA_API_URL` makes the local adapter forward business actions
  to direct REST routes such as `POST {SOMA_API_URL}/v1/echo` and
  `GET {SOMA_API_URL}/v1/status`; `SOMA_API_KEY` is sent as bearer auth
  when set.
- HTTP MCP remains available from `soma-server serve` for Docker, remote
  clients, gateway catalogs, and full platform deployments.
- Plugin manifests must not auto-register HTTP health monitors by default.
  HTTP MCP and health monitor use are explicit remote/gateway choices, not the
  local plugin default.

## Contract

The normative profile contract lives in
[`docs/contracts/plugin-stdio-adapter.md`](../contracts/plugin-stdio-adapter.md).

The short version:

- `example` must provide CLI commands and `soma mcp`.
- `soma-server` must provide direct `/v1/*` business routes, `/v1/capabilities`,
  `/mcp`, `/health`, `/status`, `/openapi.json`, and the optional web/static
  surface.
- The stdio adapter calls the business REST API, not the MCP protocol endpoint.
- Shared plugin validation must assert the stdio config and run a stdio smoke
  test.

## Consequences

Positive:

- Local plugin installs do not require a local HTTP daemon.
- Upstream-client servers stay lightweight.
- Platform servers keep one full deployment shape while still supporting a
  local plugin adapter.
- Gateway/shared deployments can continue to use Streamable HTTP MCP.

Tradeoffs:

- Platform-style plugin installs depend on the deployed API URL when they need
  real data.
- Direct REST business routes become part of the adapter contract.
- Gemini and Claude/Codex use different variable interpolation syntax, so the
  validator must check both manifests.

## Alternatives considered

### HTTP-first plugin config

Rejected. It forces local plugin users to run or reach an HTTP MCP server before
the plugin can work. That is heavier than needed for upstream-client servers and
awkward for platform servers that already expose a deployed API.

### Single full binary everywhere

Rejected. Shipping API, Web, HTTP MCP, auth, and static assets in every local
plugin install makes upstream-client servers unnecessarily large and blurs the
line between local adapter and platform deployment.

### Stdio adapter forwards to remote MCP

Rejected. The adapter should call the platform's business API contract. Using
MCP as the adapter-to-server protocol would stack MCP over MCP, complicate
errors/auth, and make the REST/API surface less useful for non-MCP consumers.

## References

- [`README.md`](../../README.md)
- [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md)
- [`docs/DEPLOYMENT.md`](../DEPLOYMENT.md)
- [`docs/PLUGINS.md`](../PLUGINS.md)
- [`scripts/check-plugin-stdio-smoke.sh`](../../scripts/check-plugin-stdio-smoke.sh)
- [`scripts/validate-plugin-layout.sh`](../../scripts/validate-plugin-layout.sh)
