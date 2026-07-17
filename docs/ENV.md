---
title: "Environment Variables"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
upstream_refs:
  - "crates/soma/config/src/env_registry.rs"
  - "crates/soma/config/src/config.rs"
last_reviewed: "2026-06-19"
---

# Environment variables

This file is generated from `ENV_KEY_SPECS` and typed config defaults. Run `cargo xtask generate-docs` after changing env/config metadata.

## Runtime variables

| Variable | Default | Secret | TOML destination | Plugin option | Purpose |
|---|---:|---:|---|---|---|
| `SOMA_API_URL` | unset | no | `soma.api_url` | `CLAUDE_PLUGIN_OPTION_SOMA_API_URL` | Deployed platform API or upstream API base URL used by `SomaClient`. Empty selects offline stub mode. |
| `SOMA_API_KEY` | unset | yes | `soma.api_key` | `CLAUDE_PLUGIN_OPTION_SOMA_API_KEY` | Bearer token or upstream API key. Keep secret. Required when the deployed API requires auth. |
| `SOMA_MCP_TOKEN` | unset | yes | `mcp.api_token` | `CLAUDE_PLUGIN_OPTION_API_TOKEN` | Static bearer token. Required for bearer-only mounted HTTP. |
| `SOMA_SERVER_URL` | unset | no | - | `CLAUDE_PLUGIN_OPTION_SERVER_URL` | Optional remote/platform HTTP server URL used by plugin setup and health checks. |
| `SOMA_MCP_AUTH_MODE` | `bearer` | no | `mcp.auth.mode` | `CLAUDE_PLUGIN_OPTION_AUTH_MODE` | `bearer` or `oauth`. |
| `SOMA_MCP_NO_AUTH` | `false` | no | `mcp.no_auth` | `CLAUDE_PLUGIN_OPTION_NO_AUTH` | Disable local auth for loopback development only. |
| `SOMA_NOAUTH` | `false` | no | `mcp.trusted_gateway` | - | Trusted-gateway no-auth mode for non-loopback deployments where an upstream proxy enforces auth. |
| `SOMA_MCP_PUBLIC_URL` | unset | no | `mcp.auth.public_url` | `CLAUDE_PLUGIN_OPTION_PUBLIC_URL` | Public URL used for OAuth metadata endpoints. |
| `SOMA_MCP_GOOGLE_CLIENT_ID` | unset | yes | `mcp.auth.google_client_id` | `CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID` | Google OAuth client ID. |
| `SOMA_MCP_GOOGLE_CLIENT_SECRET` | unset | yes | `mcp.auth.google_client_secret` | `CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET` | Google OAuth client secret. |
| `SOMA_MCP_AUTH_ADMIN_EMAIL` | unset | no | `mcp.auth.admin_email` | `CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL` | Initial/admin email allowed by the OAuth flow. |
| `SOMA_MCP_HOST` | `127.0.0.1` | no | `mcp.host` | - | Bind host for HTTP transport. Set `0.0.0.0` only with bearer, OAuth, or trusted-gateway auth configured. |
| `SOMA_MCP_SERVER_NAME` | `soma` | no | `mcp.server_name` | - | MCP server name advertised to clients. |
| `SOMA_MCP_PORT` | `40060` | no | `mcp.port` | - | Bind port for HTTP transport. |
| `SOMA_MCP_ALLOWED_HOSTS` | unset | no | `mcp.allowed_hosts` | - | Extra accepted Host header values, comma-separated. |
| `SOMA_MCP_ALLOWED_ORIGINS` | unset | no | `mcp.allowed_origins` | - | Extra CORS origins, comma-separated. |

## Docker runtime

| Variable | Purpose |
|---|---|
| `PUID` | UID to run the container as (default: 1000). |
| `PGID` | GID to run the container as (default: 1000). |
| `DOCKER_NETWORK` | Docker network name (default: `mcp`). |
| `VERSION` | Image tag to pull (default: `latest`). |

## Logging

| Variable | Example | Purpose |
|---|---|---|
| `RUST_LOG` | `info,rmcp=warn` | Tracing filter. |
| `NO_COLOR` | `1` | Disable ANSI color in console logs. |
| `FORCE_COLOR` | `1` | Force ANSI color even when stderr is not a TTY. |

## Safety

`.env` and `.env.*` are ignored by `.gitignore` and blocked by `scripts/block-env-commits.sh`. Only `.env.example` belongs in git.

Non-secret settings go in `config.toml`; secrets and deployment URLs go in `.env`. See `docs/CONFIG.md` for the full split.

Generate a bearer token:

```bash
just gen-token
# or: openssl rand -hex 32
```
