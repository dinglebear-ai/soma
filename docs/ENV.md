---
title: "Environment Variables"
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
| `SOMA_PYTHON_BROKER_ALLOWED_HTTP_HOSTS` | unset | no | `python.broker_allowed_http_hosts` | - | CUSTOMIZE: document this environment variable. |
| `SOMA_PYTHON_BROKER_SECRET_NAMES` | unset | no | `python.broker_secret_names` | - | CUSTOMIZE: document this environment variable. |
| `SOMA_PYTHON_BROKER_STATE_NAMESPACES` | unset | no | `python.broker_state_namespaces` | - | CUSTOMIZE: document this environment variable. |
| `SOMA_PYTHON_BROKER_ALLOW_LOGGING` | unset | no | `python.broker_allow_logging` | - | CUSTOMIZE: document this environment variable. |
| `SOMA_PYTHON_BROKER_ALLOW_METRICS` | unset | no | `python.broker_allow_metrics` | - | CUSTOMIZE: document this environment variable. |
| `SOMA_PYTHON_BROKER_ALLOW_PROGRESS` | unset | no | `python.broker_allow_progress` | - | CUSTOMIZE: document this environment variable. |
| `SOMA_PYTHON_BROKER_CGROUP_ROOT` | unset | no | - | - | CUSTOMIZE: document this environment variable. |
| `SOMA_PYTHON_EXECUTION_PROFILE` | unset | no | `python.execution_profile` | - | CUSTOMIZE: document this environment variable. |
| `SOMA_API_URL` | unset | no | `soma.api_url` | `CLAUDE_PLUGIN_OPTION_SOMA_API_URL` | Deployed platform API or upstream API base URL used by `SomaClient`. Empty selects offline stub mode. |
| `SOMA_API_KEY` | unset | yes | `soma.api_key` | `CLAUDE_PLUGIN_OPTION_SOMA_API_KEY` | Bearer token or upstream API key. Keep secret. Required when the deployed API requires auth. |
| `SOMA_PYTHON_ENVIRONMENT_ENABLED` | `false` | no | `python.environment.enabled` | - | Enable immutable PEP 723 Python environments. Disabled by default; all companion identity fields are required when enabled. |
| `SOMA_PYTHON_ENVIRONMENT_CACHE_ROOT` | unset | no | `python.environment.cache_root` | - | Absolute root for content-addressed Python environment caches. |
| `SOMA_PYTHON_ENVIRONMENT_UV_PROGRAM` | unset | no | `python.environment.uv_program` | - | Absolute path to the exact `uv` executable used to lock and synchronize environments. |
| `SOMA_PYTHON_ENVIRONMENT_UV_VERSION` | unset | no | `python.environment.uv_version` | - | Version identity of the configured `uv` executable; included in cache keys. |
| `SOMA_PYTHON_ENVIRONMENT_PYTHON_EXECUTABLE` | unset | no | `python.environment.python_executable` | - | Absolute path to the Python interpreter used to create environments. |
| `SOMA_PYTHON_ENVIRONMENT_RUNTIME_IMPLEMENTATION` | unset | no | `python.environment.runtime_implementation` | - | Configured interpreter implementation identity, such as `cpython`. |
| `SOMA_PYTHON_ENVIRONMENT_RUNTIME_VERSION` | unset | no | `python.environment.runtime_version` | - | Configured interpreter version identity, such as `3.12.4`. |
| `SOMA_PYTHON_ENVIRONMENT_RUNTIME_PLATFORM` | unset | no | `python.environment.runtime_platform` | - | Configured interpreter platform identity, such as `linux-x86_64`. |
| `SOMA_PYTHON_ENVIRONMENT_WHEEL_PLATFORM_TAG` | unset | no | `python.environment.wheel_platform_tag` | - | Wheel platform tag required for the configured interpreter, such as `manylinux_2_17_x86_64`. |
| `SOMA_PYTHON_ENVIRONMENT_SDK_WHEEL` | unset | no | `python.environment.sdk_wheel` | - | Absolute path to the exact Soma SDK/native wheel installed into each environment. |
| `SOMA_PYTHON_ENVIRONMENT_SDK_WHEEL_SHA256` | unset | no | `python.environment.sdk_wheel_sha256` | - | Expected 64-hex SHA-256 digest of the configured SDK/native wheel. |
| `SOMA_PYTHON_ENVIRONMENT_OFFLINE` | `false` | no | `python.environment.offline` | - | Require an already-complete cache and forbid `uv` execution on a cache miss. |
| `SOMA_PYTHON_ENVIRONMENT_UPDATE` | `false` | no | `python.environment.update` | - | Resolve a new immutable candidate generation during provider preparation. |
| `SOMA_PYTHON_ENVIRONMENT_POLICY_VERSION` | `2` | no | `python.environment.policy_version` | - | Require the configured environment policy schema to match this Soma binary. |
| `SOMA_MCP_TOKEN` | unset | yes | `mcp.api_token` | `CLAUDE_PLUGIN_OPTION_API_TOKEN` | Static bearer token. Required for bearer-only mounted HTTP. |
| `SOMA_SERVER_URL` | unset | no | - | `CLAUDE_PLUGIN_OPTION_SERVER_URL` | Optional remote/platform HTTP server URL used by plugin setup and health checks. |
| `SOMA_MCP_AUTH_MODE` | `bearer` | no | `mcp.auth.mode` | `CLAUDE_PLUGIN_OPTION_AUTH_MODE` | `bearer` or `oauth`. |
| `SOMA_MCP_NO_AUTH` | `false` | no | `mcp.no_auth` | `CLAUDE_PLUGIN_OPTION_NO_AUTH` | Disable local auth for loopback development only. |
| `SOMA_NOAUTH` | `false` | no | `mcp.trusted_gateway` | - | Trusted-gateway no-auth mode for non-loopback deployments where an upstream proxy enforces auth. |
| `SOMA_MCP_PUBLIC_URL` | unset | no | `mcp.auth.public_url` | `CLAUDE_PLUGIN_OPTION_PUBLIC_URL` | Public URL used for OAuth metadata endpoints. |
| `SOMA_MCP_GOOGLE_CLIENT_ID` | unset | yes | `mcp.auth.google_client_id` | `CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID` | Google OAuth client ID. |
| `SOMA_MCP_GOOGLE_CLIENT_SECRET` | unset | yes | `mcp.auth.google_client_secret` | `CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET` | Google OAuth client secret. |
| `SOMA_MCP_GOOGLE_CALLBACK_PATH` | `/auth/google/callback` | no | `mcp.auth.google_callback_path` | - | Google callback path (default: `/auth/google/callback`). |
| `SOMA_MCP_GOOGLE_SCOPES` | `openid,email,profile` | no | `mcp.auth.google_scopes` | - | Comma-separated Google scopes (default: `openid,email,profile`). |
| `SOMA_MCP_AUTHELIA_ISSUER_URL` | unset | no | `mcp.auth.authelia_issuer_url` | - | Authelia OIDC issuer URL. HTTPS is required. |
| `SOMA_MCP_AUTHELIA_CLIENT_ID` | unset | yes | `mcp.auth.authelia_client_id` | - | Authelia OIDC client ID. |
| `SOMA_MCP_AUTHELIA_CLIENT_SECRET` | unset | yes | `mcp.auth.authelia_client_secret` | - | Authelia OIDC client secret. |
| `SOMA_MCP_AUTHELIA_CALLBACK_PATH` | `/auth/authelia/callback` | no | `mcp.auth.authelia_callback_path` | - | Authelia callback path (default: `/auth/authelia/callback`). |
| `SOMA_MCP_AUTHELIA_SCOPES` | `openid,email,profile,offline_access` | no | `mcp.auth.authelia_scopes` | - | Comma-separated Authelia scopes (default: `openid,email,profile,offline_access`). |
| `SOMA_MCP_GITHUB_CLIENT_ID` | unset | yes | `mcp.auth.github_client_id` | - | GitHub OAuth App client ID. |
| `SOMA_MCP_GITHUB_CLIENT_SECRET` | unset | yes | `mcp.auth.github_client_secret` | - | GitHub OAuth App client secret. |
| `SOMA_MCP_GITHUB_CALLBACK_PATH` | `/auth/github/callback` | no | `mcp.auth.github_callback_path` | - | GitHub callback path (default: `/auth/github/callback`). |
| `SOMA_MCP_GITHUB_SCOPES` | `read:user,user:email` | no | `mcp.auth.github_scopes` | - | Comma-separated GitHub scopes; must include `user:email` (default: `read:user,user:email`). |
| `SOMA_MCP_AUTH_DEFAULT_PROVIDER` | `automatic` | no | `mcp.auth.default_provider` | - | Provider used when `provider` is omitted: `google`, `authelia`, or `github`; automatic priority is Google, Authelia, GitHub. |
| `SOMA_MCP_AUTH_ADMIN_EMAIL` | unset | no | `mcp.auth.admin_email` | `CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL` | Initial/admin email allowed by the OAuth flow. |
| `SOMA_MCP_HOST` | `127.0.0.1` | no | `mcp.host` | - | Bind host for HTTP transport. Set `0.0.0.0` only with bearer, OAuth, or trusted-gateway auth configured. |
| `SOMA_MCP_SERVER_NAME` | `soma` | no | `mcp.server_name` | - | MCP server name advertised to clients. |
| `SOMA_MCP_PORT` | `40060` | no | `mcp.port` | - | Bind port for HTTP transport. |
| `SOMA_MCP_ALLOWED_HOSTS` | unset | no | `mcp.allowed_hosts` | - | Extra accepted Host header values, comma-separated. |
| `SOMA_MCP_ALLOWED_ORIGINS` | unset | no | `mcp.allowed_origins` | - | Extra CORS origins, comma-separated. |
| `SOMA_MCP_TRACE_HEADERS` | `off` | no | `mcp.trace_headers` | `CLAUDE_PLUGIN_OPTION_TRACE_HEADERS` | Trusted inbound HTTP trace-header extraction: `off`, `trusted`, or `trusted-with-baggage`. Enable only behind a transport-level trust boundary; see `docs/TRACE_CONTEXT.md`. |
| `SOMA_MCP_STATIC_TOKEN_WRITE` | unset | no | `mcp.static_token_write` | - | Grant the static bearer token `soma:write` in addition to `soma:read`. Read-only by default. |
| `SOMA_MCP_AUTH_BOOTSTRAP_SECRET` | unset | yes | `mcp.auth.bootstrap_secret` | - | Native-flow bootstrap secret for the desktop/CLI OAuth poll flow. |
| `SOMA_MCP_AUTH_SQLITE_PATH` | unset | no | `mcp.auth.sqlite_path` | - | Auth SQLite DB path. Unset uses the built-in default under the data dir. |
| `SOMA_MCP_AUTH_KEY_PATH` | unset | no | `mcp.auth.key_path` | - | Ed25519 JWT signing key path. Unset uses the built-in default under the data dir. |
| `SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS` | unset | no | `mcp.auth.access_token_ttl_secs` | - | Access-token lifetime in seconds. Unset uses the built-in auth default. |
| `SOMA_MCP_AUTH_REFRESH_TOKEN_TTL_SECS` | unset | no | `mcp.auth.refresh_token_ttl_secs` | - | Refresh-token lifetime in seconds. Unset uses the built-in auth default. |
| `SOMA_MCP_AUTH_CODE_TTL_SECS` | unset | no | `mcp.auth.auth_code_ttl_secs` | - | Authorization-code lifetime in seconds. Unset uses the built-in auth default. |
| `SOMA_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE` | unset | no | `mcp.auth.register_rpm` | - | Per-IP `/register` rate limit. Unset uses the built-in auth default. |
| `SOMA_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE` | unset | no | `mcp.auth.authorize_rpm` | - | Per-IP `/authorize` rate limit. Unset uses the built-in auth default. |
| `SOMA_MCP_AUTH_MAX_PENDING_OAUTH_STATES` | unset | no | `mcp.auth.max_pending_oauth_states` | - | Cap on pending OAuth state rows (DoS bound). Unset uses the built-in auth default. |
| `SOMA_MCP_AUTH_ALLOWED_REDIRECT_URIS` | unset | no | `mcp.auth.allowed_client_redirect_uris` | - | Comma-separated allowlist of dynamic-client redirect URIs. |
| `SOMA_MCP_TOKEN_ENCRYPTION_KEY` | unset | yes | `mcp.auth.token_encryption_key` | - | At-rest encryption key for stored provider refresh tokens (64-hex or 43-char base64url). Keep secret. |

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
