---
title: "Plugin stdio adapter contract"
doc_type: "contract"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "template"
source_of_truth: true
upstream_refs:
  - "plugins/rtemplate/.mcp.json"
  - "plugins/rtemplate/gemini-extension.json"
  - "crates/rtemplate-service/src/example.rs"
  - "crates/rmcp-template/src/bin/example.rs"
  - "crates/rmcp-template/src/main.rs"
last_reviewed: "2026-05-26"
---

# Plugin stdio adapter contract

This contract records the behavior required by
[`ADR 0001`](../adr/0001-stdio-first-plugin-adapter.md). It is the stable
checklist for template adapters and for services scaffolded from this repo.

## Required profiles

| Profile | Binary | Required surfaces | Intended use |
|---|---|---|---|
| Local adapter | `example` | CLI + stdio MCP | Plugin installs, local scripting, parity tests |
| Full server | `example-server` | REST API + Web + Streamable HTTP MCP + health/auth | Docker, systemd, gateway, remote clients |

## Plugin MCP config

Claude Code and Codex share `plugins/rtemplate/.mcp.json`. The default MCP server
entry must be stdio-first:

```json
{
  "type": "stdio",
  "command": "example",
  "args": ["mcp"],
  "env": {
    "RTEMPLATE_API_URL": "${user_config.rtemplate_api_url}",
    "RTEMPLATE_API_KEY": "${user_config.rtemplate_api_key}",
    "RUST_LOG": "warn"
  }
}
```

Gemini must use the same local adapter behavior:

```json
{
  "command": "example",
  "args": ["mcp"],
  "env": {
    "RTEMPLATE_API_URL": "${settings.rtemplate_api_url}",
    "RTEMPLATE_API_KEY": "${settings.rtemplate_api_key}",
    "RUST_LOG": "warn"
  }
}
```

Plugin manifests must not auto-register an HTTP MCP health monitor by default.
If a derived service needs remote/gateway HTTP MCP, document that as an explicit
operator setting rather than the plugin install default.

## Adapter API contract

The local adapter resolves its runtime mode from `RTEMPLATE_API_URL`:

| `RTEMPLATE_API_URL` | Behavior |
|---|---|
| empty | Offline template stub mode. Used for local smoke tests and scaffold examples. |
| set | Forward local CLI and stdio MCP business actions to the deployed API. |

When forwarding, the adapter must:

- preserve any base path in `RTEMPLATE_API_URL`;
- call the action's direct REST route, such as `POST {RTEMPLATE_API_URL}/v1/echo` or `GET {RTEMPLATE_API_URL}/v1/status`;
- send `RTEMPLATE_API_KEY` as `Authorization: Bearer <token>` when set;
- send business-action JSON, not MCP protocol JSON;
- surface execution failures through the existing CLI/MCP error policy.

Direct REST body shapes are action-specific:

```json
{
  "message": "hello"
}
```

REST is direct-route-only. Adapters call routes such as
`POST {RTEMPLATE_API_URL}/v1/echo` or `GET {RTEMPLATE_API_URL}/v1/status`;
there is no `/v1/example` action envelope.

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
cargo check --bin rtemplate --no-default-features --features local-adapter
cargo check --bin rtemplate-server --features full
bash scripts/check-plugin-stdio-smoke.sh
bash scripts/validate-plugin-layout.sh
cargo test --test plugin_contract
```

For release-level validation, also run the normal template gates:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
bash scripts/test-template-features.sh
cargo fmt --check
git diff --check
```
