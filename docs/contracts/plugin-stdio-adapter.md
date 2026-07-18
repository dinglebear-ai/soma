---
title: "Plugin stdio adapter contract"
doc_type: "contract"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: true
upstream_refs:
  - "plugins/soma/.claude-plugin/plugin.json"
  - "plugins/soma/.codex-plugin/plugin.json"
  - "plugins/soma/gemini-extension.json"
  - "crates/soma/client/src/client.rs"
  - "apps/soma/src/bin/soma.rs"
last_reviewed: "2026-05-26"
---

# Plugin stdio adapter contract

This contract records the behavior required by
[`ADR 0001`](../adr/0001-stdio-first-plugin-adapter.md). It is the stable
checklist for Soma adopters and for services scaffolded from this repo.

## Required runtime modes

| Mode | Command | Required surfaces | Intended use |
|---|---|---|---|
| Local adapter | `soma mcp` and `soma <command>` | CLI + stdio MCP | Plugin installs, local scripting, parity tests |
| HTTP runtime | `soma serve` | REST API + Web + Streamable HTTP MCP + health/auth | Docker, systemd, gateway, remote clients |

## Plugin MCP config

Claude Code and Codex package metadata lives in their platform manifests; live
MCP registration is supplied by the client or gateway. The default MCP server
entry must be stdio-first when configured:

```json
{
  "type": "stdio",
  "command": "soma",
  "args": ["mcp"],
  "env": {
    "SOMA_API_URL": "${user_config.soma_api_url}",
    "SOMA_API_KEY": "${user_config.soma_api_key}",
    "RUST_LOG": "warn"
  }
}
```

Gemini must use the same local adapter behavior:

```json
{
  "command": "soma",
  "args": ["mcp"],
  "env": {
    "SOMA_API_URL": "${settings.soma_api_url}",
    "SOMA_API_KEY": "${settings.soma_api_key}",
    "RUST_LOG": "warn"
  }
}
```

Plugin manifests must not auto-register an HTTP MCP health monitor by default.
If a derived service needs remote/gateway HTTP MCP, document that as an explicit
operator setting rather than the plugin install default.

## Adapter API contract

The local adapter resolves its runtime mode from `SOMA_API_URL`:

| `SOMA_API_URL` | Behavior |
|---|---|
| empty | Local provider/static dispatch. Used for local smoke tests and scaffold examples. |
| set | Forward local CLI and stdio MCP business actions to the deployed API. |

When forwarding, the adapter must:

- preserve any base path in `SOMA_API_URL`;
- call the action's direct REST route, such as `POST {SOMA_API_URL}/v1/echo` or `GET {SOMA_API_URL}/v1/status`;
- send `SOMA_API_KEY` as `Authorization: Bearer <token>` when set;
- send business-action JSON, not MCP protocol JSON;
- surface execution failures through the existing CLI/MCP error policy.

Direct REST body shapes are action-specific:

```json
{
  "message": "hello"
}
```

Adapters should call direct REST routes such as `GET {SOMA_API_URL}/v1/status`
or `POST {SOMA_API_URL}/v1/echo`. REST does not expose an action envelope:

```json
{
  "message": "hello"
}
```

The MCP tool argument shape remains:

```json
{
  "action": "status"
}
```

## Verification commands

Run these after changing binary profiles, plugin manifests, adapter behavior, or
transport docs:

```bash
cargo check --bin soma --no-default-features --features local-adapter
cargo check --bin soma --features full
bash scripts/check-plugin-stdio-smoke.sh
bash scripts/validate-plugin-layout.sh
cargo test --test plugin_contract
```

For release-level validation, also run the normal Soma gates:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
bash scripts/test-soma-features.sh
cargo fmt --check
git diff --check
```
