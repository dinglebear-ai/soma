---
title: "Deployment"
created: 2026-05-15
updated: 2026-07-30
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
last_reviewed: "2026-05-15"
---

# Deployment

Soma supports three deployment modes:

1. **Local development** with `just dev`.
2. **Docker Compose** with `just docker-up`.
3. **User systemd** with an installed release binary.

## Deployment profile choice

Choose the runtime shape from the server category:

| Server kind | Preferred deployment |
|---|---|
| Upstream-client MCP server | Installed local binary that exposes CLI + stdio MCP and calls the upstream API directly. |
| Application/platform server | Docker/systemd deployment that runs `soma serve` for API + Web + HTTP MCP, with optional local CLI/stdio adapter behavior. |
| Gateway-shared tool | HTTP MCP deployment retained for gateway/catalog access. |

Do not add a local REST/Web mirror only because an upstream service has an HTTP
API. Add REST/Web when this project owns state, background work, dashboards, or
non-MCP consumers.

See [`docs/adr/0001-stdio-first-plugin-adapter.md`](adr/0001-stdio-first-plugin-adapter.md)
for the accepted decision and
[`docs/contracts/plugin-stdio-adapter.md`](contracts/plugin-stdio-adapter.md)
for the deployable profile contract.

## Binary command surface

The all-in-one Soma binary exposes explicit runtime modes. If a derived server
uses narrower feature sets, keep the command names stable on the profile where
they apply:

| Command | Mode | Description |
|---|---|---|
| `soma mcp` | stdio MCP | For Claude Code `~/.claude/settings.json` stdio servers |
| `soma serve` | HTTP runtime | Owns the provider registry, REST `/v1/*`, web UI, health/status, auth, and Streamable HTTP MCP at `/mcp`. |
| `soma [subcommand]` | CLI/API adapter | Uses local provider/static dispatch by default. With remote mode or `SOMA_API_URL` set, targets the deployed `soma` REST API. |
| `soma doctor` | Pre-flight check | Validates environment and config |
| `soma --help` | Help | Print usage |
| `soma --version` | Version | Print version |

## Deployment checklist

1. Build and test locally:
   ```bash
   just verify
   cargo xtask pre-release-check
   ```
2. Create a `.env` from `.env.example` and set real credentials.
3. Generate a bearer token:
   ```bash
   just gen-token
   ```
4. Choose Docker or systemd.
5. Verify runtime freshness:
   ```bash
   just runtime-current
   ```
6. Smoke-test auth:
   ```bash
   SOMA_MCP_TOKEN=<token> just auth-smoke
   ```
7. Run MCP integration tests:
   ```bash
   just test-mcporter
   ```

The production Compose sample enables trusted-gateway mode because local
Python providers deliberately reject ordinary remote bearer-token execution.
It publishes Soma on host loopback by default; place an authenticating and
authorizing reverse proxy on that host in front of it:

```bash
docker compose -f docker-compose.prod.yml config --quiet
docker compose -f docker-compose.prod.yml up -d
curl --fail http://127.0.0.1:40060/health
```

`SOMA_MCP_PUBLISH_HOST` controls the host-side bind address and defaults to
`127.0.0.1`. A Tailscale address may be set in the project `.env` when tailnet
identity and ACLs are the intended transport trust boundary. Never set it to
`0.0.0.0` or an unprotected LAN address while `SOMA_NOAUTH=true`. Deployments
without local-runtime providers may instead remove `SOMA_NOAUTH`, set
`SOMA_MCP_TOKEN`, and publish the bearer-protected service directly.

## Binary environment awareness

The binary normalizes data paths based on its deployment context. Bind host and
port come from typed config and environment variables; Docker deployments must
set `SOMA_MCP_HOST=0.0.0.0` explicitly when exposing the service outside the
container.

```rust
fn is_containerized() -> bool {
    std::path::Path::new("/.dockerenv").exists()
        || std::env::var("RUNNING_IN_CONTAINER").is_ok()
        || std::env::var("container").is_ok()
}

fn resolve_data_dir(config_path: Option<&str>) -> PathBuf {
    if let Some(p) = config_path { return PathBuf::from(p); }
    if is_containerized() { return PathBuf::from("/data"); }
    dirs::home_dir().unwrap_or_default().join(".example")
}

```

## Appdata convention

All deployments share `~/.<service>` as the logical data root:

| Deployment | Data directory |
|---|---|
| Local binary | `~/.soma/` |
| Docker | `/data/` in container, mounted from `~/.soma/` on host |
| Plugin | `$CLAUDE_PLUGIN_DATA` (symlinked to `~/.soma/`) |

## Auth expectations

Non-loopback HTTP deployments must use bearer auth or OAuth. The server refuses to bind to a non-loopback address without authentication unless explicitly configured:

- Loopback bind → `LoopbackDev` (no auth)
- `SOMA_MCP_NO_AUTH=true` → valid only on loopback
- Non-loopback + bearer token → mounted bearer auth
- Non-loopback + `auth_mode=oauth` → mounted OAuth auth
- Non-loopback + `SOMA_NOAUTH=true` → `TrustedGatewayUnscoped` (trusted gateway, explicit opt-out)
- Non-loopback + no credentials + no gateway acknowledgment → startup error

## Claude Code stdio config

```json
{
  "mcpServers": {
    "soma": {
      "type": "stdio",
      "command": "soma",
      "args": ["mcp"]
    }
  }
}
```

The binary must be in `$PATH`. The plugin hook (`<binary> setup plugin-hook`) self-installs it to `~/.local/bin/` on SessionStart.

## Public endpoints

- `/health` is public and fast.
- `/status` is public but redacted.
- `/mcp` is the Streamable HTTP MCP endpoint.
- `/v1/*` includes direct native business routes and provider-backed dynamic REST routes.
- REST supports native direct routes, `POST /v1/tools/{action}`, provider inventory routes, and provider-declared dynamic paths. MCP keeps action dispatch behind the single `/mcp` tool surface.

## Port assignments

Each service in the rmcp family uses a fixed port to avoid collisions:

| Service | MCP Port | Binary name |
|---|---|---|
| labby | 8765 | `labby` |
| axon | 8001 | `axon` |
| cortex | 3100 | `cortex` |
| unraid-rmcp | 6970 | `runraid` |
| gotify-rmcp | 9158 | `rgotify` |
| unifi-rmcp | 7474 | `runifi` |
| tailscale-rmcp | 7575 | `rtailscale` |
| apprise-rmcp | 8765 | `rapprise` |
| soma | 40060 | `soma` |

Set the port via `SOMA_MCP_PORT` or in `config.toml`. Update `EXPOSE` in the Dockerfile and the port mapping in `docker-compose.yml` to match.

## Worktree file propagation

Claude Code worktrees are fresh checkouts — gitignored files like `.env` and `config.toml` are absent by default. The `.worktreeinclude` file at the repo root tells Claude Code which gitignored files to copy into each new worktree automatically:

```
# .worktreeinclude
.env
config.toml
```

This ensures the server can start in a worktree without manual setup. Both files are one-way copied (main → worktree) at worktree creation time only.

`.gitignore` additions required alongside `.worktreeinclude`:

```gitignore
config.toml
.beagle/
```

See `docs/DOCKER.md`, `docs/SYSTEMD.md`, `docs/ENV.md`, and `docs/CONFIG.md` for deployment-specific details. See `docs/PATTERNS.md` §19, §27, §28, §46, §47, §A6 for port assignments, security, environment awareness, binary installation, and worktree patterns.
