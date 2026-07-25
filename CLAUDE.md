# soma — Claude Code instructions

## What this project is

Soma is a batteries-included Rust RMCP runtime for shipping MCP servers with
drop-in tools, prompts, resources, auth, docs, plugins, web fallback, and
release automation already wired. The canonical binary is `soma`; explicit
subcommands select HTTP server, stdio MCP, or CLI adapter mode.

## Long-Lived Branches

- `marketplace-no-mcp` is a protected long-lived marketplace variant branch,
  not stale cleanup. This protection applies in every repo, not only this one.
- Never merge, rebase, delete, prune, squash, cherry-pick away, remove the
  worktree for, or otherwise "clean up" any `marketplace-no-mcp` branch unless
  Jacob explicitly names that exact branch and says to retire/delete it.
- Broad cleanup requests such as "clean everything", "down to just main/main",
  "prune stale branches", "merge everything back", or "remove old worktrees" do
  not apply to `marketplace-no-mcp`. Treat it as off limits and leave both the
  local/remote branch and its worktree intact.
- The branch keeps the Soma/plugin skill surface available while removing
  bundled MCP server registration for environments where the MCP server is
  already connected through the Labby gateway.

## Module map

| File | Role |
|------|------|
| `crates/soma/client/src/client.rs` | `SomaClient` — HTTP/API transport stub; one method per remote operation |
| `crates/soma/application/src/app.rs` | `SomaApplication` — shared use-case facade every surface (MCP/REST/CLI) calls |
| `crates/soma/application/src/service.rs` | `SomaService` — business layer; all logic lives here, never in shims |
| `crates/soma/application/src/provider_registry.rs` | `ProviderRegistry` — provider dispatch, catalog snapshots, resource/prompt indexing |
| `crates/soma/application/src/providers/` | Provider sources: static Rust, file-backed manifests, remote catalogs |
| `crates/soma/domain/src/actions.rs` | `SomaAction`, `ACTION_SPECS` — canonical action metadata, scope/transport availability |
| `crates/soma/domain/src/errors.rs` | `ServiceError`, `ToolError` — error taxonomy shared across surfaces |
| `crates/soma/config/src/config.rs` | `Config`, `SomaConfig`, `McpConfig`, `AuthConfig`, env loading |
| `crates/soma/integrations/` | Product bridges from `SomaApplication` ports to shared engines (gateway, auth) |
| `crates/soma/runtime/src/server.rs` | `SomaRuntime`, `AppState`, `AuthPolicy`, `build_auth_layer` — process facade, HTTP state, and auth policy |
| `apps/soma/src/routes.rs` | Axum router: `/mcp`, `/health`, `/status`, OAuth discovery routes |
| `crates/soma/api/src/api.rs` | REST API handlers: direct `/v1/*` routes, `GET /health`, `GET /status` |
| `crates/soma/mcp/src/lib.rs` | MCP protocol layer — re-exports from `mcp/` submodules |
| `crates/soma/mcp/src/tools.rs` | MCP shim: parse JSON args → call `SomaApplication` → return `Value` |
| `crates/soma/mcp/src/schemas.rs` | Tool JSON schema derived from `ACTION_SPECS` |
| `crates/soma/mcp/src/rmcp_server.rs` | `ServerHandler` impl: tools, resources, prompts, scope checks |
| `crates/soma/mcp/src/prompts.rs` | MCP prompts (`quick_start`) |
| `crates/soma/cli/src/lib.rs` | CLI shim: parse args → call `SomaApplication` → print |
| `crates/soma/cli/src/doctor.rs` | Pre-flight checks: env, connectivity, config validation |
| `crates/soma/cli/src/setup.rs` | Interactive first-run / plugin setup wizard |
| `crates/soma/cli/src/watch.rs` | Polls `/health` and emits state-change lines for plugin monitor |
| `crates/soma/mcp/src/transport.rs` | Streamable HTTP transport wiring and session lifecycle |
| `crates/soma/domain/src/token_limit.rs` | Token budget enforcement for MCP response payloads |
| `apps/soma/src/bin/soma.rs` | Canonical binary dispatcher for `serve`, `mcp`, and CLI modes |
| `apps/soma/src/lib.rs` | Public facade + `testing` helpers for integration tests |
| `apps/soma/tests/cli_parse.rs` | CLI argument parsing tests |
| `apps/soma/tests/tool_dispatch.rs` | MCP tool dispatch tests (application-layer, no real credentials) |

## The thin-shim rule — enforce this hard

`crates/soma/mcp/src/tools.rs` and `crates/soma/cli/src/lib.rs` contain **zero business logic**. They only:
1. Parse their input format (JSON args or CLI flags)
2. Call the corresponding `SomaApplication` action
3. Return the result

If you find yourself computing, filtering, transforming, or validating data in
`tools.rs` or `cli.rs`, stop and move it behind the application facade.

Dynamic providers load from `./providers` by default or `SOMA_PROVIDER_DIR`. Two distinct CLI surfaces inspect them:
- `soma providers validate|inspect|test` — dispatches through the *live, loaded* `ProviderRegistry`; executes handlers.
- `soma providers list|lint|status` — non-executing filesystem inspection via `soma_application::providers::filesystem::FileProviderSource::inspect()`; never loads the registry or runs TS/WASM/MCP/OpenAPI handlers. Use `soma providers lint` before committing provider examples or runtime docs.

## How to add an action

1. **`crates/soma/client/src/client.rs`** — add `pub async fn your_action(&self, ...) -> Result<Value>` with the actual HTTP/API call (or stub).

2. **`crates/soma/application/src/service.rs`** — add a delegating method: `pub async fn your_action(&self, ...) -> Result<Value> { self.client.your_action(...).await }`.

3. **`crates/soma/domain/src/actions.rs`** — add the action to `ACTION_SPECS`, including scope and transport.

4. **`crates/soma/mcp/src/schemas.rs`** — add any new parameters to `tool_definitions()`; the action enum comes from `ACTION_SPECS`.

5. **`crates/soma/mcp/src/tools.rs`** — translate the MCP request into the
   corresponding `SomaApplication` action and add any MCP-specific help text.

6. **`crates/soma/cli/src/lib.rs`** — add a `Command` variant, a parse arm in `parse_args()`, and a dispatch arm in `run()`.

7. **`apps/soma/tests/tool_dispatch.rs`** — add a test.

8. **`CHANGELOG.md`** — add an entry under `[Unreleased]` describing the new action.

For actions with parameters, extract them with `string_arg(&args, "param_name")` in `tools.rs`.

## Auth model

`AuthPolicy` is an enum with four variants:

| Variant | When | Effect |
|---------|------|--------|
| `AuthPolicy::LoopbackDev` | `no_auth=true` or host is loopback (`localhost`, `127.*`, `::1`) via `McpConfig::is_loopback()` | No auth middleware; scope checks bypassed |
| `AuthPolicy::TrustedGatewayUnscoped` | `SOMA_NOAUTH=true` on non-loopback behind an authz-enforcing gateway | No auth middleware; scope checks bypassed |
| `AuthPolicy::Mounted { auth_state: None }` | Default non-loopback | Static bearer token required |
| `AuthPolicy::Mounted { auth_state: Some(_) }` | `auth_mode = "oauth"` | Google, Authelia, and/or GitHub login + EdDSA/Ed25519 JWT issuance |

Auth is selected in `build_auth_policy()` in `main.rs`. Scopes are `soma:read` and `soma:write` (write satisfies read). `help` requires no scope. Unknown actions get `DENY_SCOPE`.

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SOMA_API_URL` | — | Upstream service base URL |
| `SOMA_API_KEY` | — | Upstream service API key |
| `SOMA_MCP_HOST` | `127.0.0.1` | Bind host |
| `SOMA_MCP_PORT` | `40060` | Bind port |
| `SOMA_MCP_NO_AUTH` | `false` | Disable auth (loopback only) |
| `SOMA_MCP_TOKEN` | — | Static bearer token |
| `SOMA_MCP_STATIC_TOKEN_WRITE` | `false` | Grant the static bearer token `soma:write` in addition to `soma:read` (read-only by default) |
| `SOMA_MCP_ALLOWED_HOSTS` | — | Extra comma-separated Host header values |
| `SOMA_MCP_ALLOWED_ORIGINS` | — | Extra comma-separated CORS origins |
| `SOMA_MCP_TRACE_HEADERS` | `off` | Trusted inbound HTTP trace extraction; non-`off` requires loopback or a header-sanitizing trusted gateway. See `docs/TRACE_CONTEXT.md`. |
| `SOMA_MCP_PUBLIC_URL` | — | Public URL for OAuth metadata endpoints |
| `SOMA_MCP_AUTH_MODE` | `bearer` | `bearer` or `oauth` |
| `SOMA_MCP_GOOGLE_CLIENT_ID` | — | Google OAuth client ID |
| `SOMA_MCP_GOOGLE_CLIENT_SECRET` | — | Google OAuth client secret |
| `SOMA_MCP_AUTHELIA_ISSUER_URL` | — | HTTPS Authelia OIDC issuer URL |
| `SOMA_MCP_AUTHELIA_CLIENT_ID` | — | Authelia OIDC client ID |
| `SOMA_MCP_AUTHELIA_CLIENT_SECRET` | — | Authelia OIDC client secret |
| `SOMA_MCP_GITHUB_CLIENT_ID` | — | GitHub OAuth App client ID |
| `SOMA_MCP_GITHUB_CLIENT_SECRET` | — | GitHub OAuth App client secret |
| `SOMA_MCP_AUTH_DEFAULT_PROVIDER` | auto | Default provider; automatic priority is Google, Authelia, GitHub |
| `SOMA_MCP_AUTH_ADMIN_EMAIL` | — | OAuth admin email |
| `RUST_LOG` | `info` | Log filter |

All OAuth env vars (`SOMA_MCP_AUTHELIA_*`, `SOMA_MCP_GITHUB_*`,
`SOMA_MCP_GOOGLE_CALLBACK_PATH`/`_SCOPES`, `SOMA_MCP_AUTH_DEFAULT_PROVIDER`,
the auth TTL/rate-limit/token-encryption keys, `SOMA_MCP_AUTH_SQLITE_PATH`,
`SOMA_MCP_AUTH_KEY_PATH`) now flow through `crates/soma/config`'s typed
`AuthConfig` struct. `crates/soma/integrations/src/auth.rs`'s
`soma_auth_config()` synthesizes a `{PREFIX}_*` var list from that typed
config and hands it to `soma_auth::AuthConfigBuilder::build_from_sources()` —
so `soma_auth` no longer reads process env in Soma's OAuth path (the
synthetic-env pattern cortex uses). Var names, defaults (which still live in
`crates/shared/auth/src/config.rs` — unset fields are omitted so the builder
applies them), and `[mcp.auth]` config.toml keys are unchanged; provider
settings can now also be set in `config.toml`.

Optional per-provider overrides not shown above (defaults match
`crates/shared/auth/src/config.rs`): `SOMA_MCP_GOOGLE_CALLBACK_PATH`,
`SOMA_MCP_GOOGLE_SCOPES`, `SOMA_MCP_AUTHELIA_CALLBACK_PATH`,
`SOMA_MCP_AUTHELIA_SCOPES`, `SOMA_MCP_GITHUB_CALLBACK_PATH`,
`SOMA_MCP_GITHUB_SCOPES`.

## Elicitation

The `elicit_name` action demonstrates MCP elicitation (spec 2025-06-18). The server calls `peer.elicit::<T>()` to ask the MCP client for user input mid-call. The type `T` must:
- Derive `JsonSchema`, `Serialize`, `Deserialize`
- Be an object (struct), not a primitive
- Be registered with `rmcp::elicit_safe!(T)`

`ElicitationError::CapabilityNotSupported` is handled gracefully — clients that don't support it get a fallback message instead of an error.

## MCP error policy

Tool-originated failures must be visible to agents as structured tool results:
`CallToolResult::structured_error(...)` with `isError: true`, `kind`,
`schema_version`, stable `code`, `tool`, `action`, optional `field`/`bad_value`,
and `remediation`. Reserve protocol `ErrorData` for auth/scope denial, unknown
MCP tool names, resource/prompt lookup, malformed protocol requests, and server
serialization defects.

## Build commands

```bash
cargo build --release     # produces target/release/soma
cargo test                # all tests
cargo clippy -- -D warnings  # lint (must pass)
cargo fmt                 # format

just dev                  # SOMA_MCP_HOST=127.0.0.1 SOMA_MCP_NO_AUTH=true cargo run -- serve mcp (loopback only, no auth)
just test                 # cargo test
just lint                 # cargo clippy -- -D warnings
just fmt                  # cargo fmt
just doc                  # cargo xtask doc — rustdoc API reference (target/doc/)
just doc-check            # cargo xtask doc --strict — RUSTDOCFLAGS="-D warnings" (CI grade)
just gen-token            # openssl rand -hex 32
just health               # curl http://localhost:40060/health | jq .
```

## Test helpers

`apps/soma/src/lib.rs` exports `testing::loopback_state()` and `testing::bearer_state(token)` (behind `features = ["test-support"]` or `cfg(test)`). Use these in integration tests — they build `AppState` without real credentials.

## CLI ↔ MCP action parity

Every action in the MCP tool must also be reachable from the CLI, and vice versa.
Both shims call the same `SomaService` methods, so parity is automatic when the
shims are complete.

**Exception — MCP-only features:** `elicit_name` and MCP resources/prompts have no
CLI equivalent. Elicitation requires a live MCP client interaction (the server asks
the user for input mid-call via `peer.elicit()`); that interaction model does not
translate to a one-shot CLI call. Resources and prompts are MCP protocol concepts
with no CLI analogue.

<!-- BEGIN GENERATED CLAUDE_PARITY_TABLE -->
<!-- Generated by scripts/generate-docs.py; do not edit by hand. -->
| Service Method | MCP Action | CLI Command | REST Route | Notes |
|---|---|---|---|---|
| `service.greet(name)` | `soma(action="greet")` | `soma greet [--name N]` | `POST /v1/greet` |  |
| `service.echo(message)` | `soma(action="echo")` | `soma echo --message <msg>` | `POST /v1/echo` |  |
| `service.status()` | `soma(action="status")` | `soma status` | `GET /v1/status` |  |
| `MCP client interaction` | `soma(action="elicit_name")` | `_MCP-only_` | _MCP-only_ | MCP-only; requires elicitation-capable client |
| `MCP elicitation wizard` | `soma(action="scaffold_intent")` | `_MCP-only_` | _MCP-only_ | MCP-only; requires elicitation-capable client |
| `built-in help` | `soma(action="help")` | `soma --help` | `GET /v1/help` |  |
<!-- END GENERATED CLAUDE_PARITY_TABLE -->
**CUSTOMIZE:** Replace this table with your service's actual actions when you adapt
Soma. The rule is: one row per service method, with both the MCP action name
and the CLI subcommand/flag documented.

## Plugin versioning

Plugin manifests (`.claude-plugin/plugin.json`, `.codex-plugin/plugin.json`, `gemini-extension.json`) do **not** contain a `version` field. The marketplace derives the version from the git commit SHA on every push — adding an explicit version causes every push to be treated as a new version and creates duplicate entries. Do not add `version` to any plugin manifest and do not run `scripts/bump-version.sh` targets against plugin manifests.

## Release versioning

`release/components.toml` is the source of truth for versioning. Soma currently has one shipped component, `soma`, covering the Rust crate/binaries, embedded web assets, Docker/runtime files, MCP registry metadata, and plugin package files. `apps/soma/Cargo.toml` package `soma` is canonical; `Cargo.lock`, `server.json`, `docs/generated/openapi.json`, and `CHANGELOG.md` must stay in parity through the manifest. Plugin manifests are listed as `json_no_version` and must remain versionless.

Use:

```bash
cargo xtask check-version-sync
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
cargo xtask release-plan --head HEAD --mode main --json
cargo xtask bump-version soma patch
```

PR mode uses the merge-base of the PR branch and `origin/main`; main mode compares against the latest matching `v*` semver tag and powers `.github/workflows/auto-tag.yml`.

## Common gotchas

- **Stdio mode suppresses logs** — `main.rs` sets log level to `warn` in stdio mode so JSON-RPC is not corrupted by log lines on stdout.
- **Scope checks run in `rmcp_server.rs`**, not in `tools.rs`. `tools.rs` only dispatches.
- **`help` action is public** — `required_scope_for("help")` returns `None`. All other actions require at least `soma:read`.
- **Default port is 40060** — set in `default_mcp_port()` in `config.rs`. Override with `SOMA_MCP_PORT`.
- **`elicit_name` is MCP-only** — elicitation requires a live client connection; it cannot be invoked from the CLI. This is the one intentional parity exception.
- **`watch`, `serve`, and `doctor` are CLI infrastructure** — they are not MCP actions and have no parity requirement. `watch` polls `/health` and emits state-change lines to stdout (used by the plugin monitor). `serve` starts the HTTP server. `doctor` runs pre-flight checks. None belong in the MCP parity table.
- **CI runs on self-hosted runners behind path-aware gates** — Linux jobs use `[self-hosted, tootie, soma]`, Windows native artifact checks use `[self-hosted, Windows, soma, steamy]`, and both `ci.yml` and `msrv.yml` route jobs through `cargo xtask changed-paths`. Branch protection should require the stable aggregate `CI Gate` and `MSRV Gate` statuses, not individual path-skipped jobs. This is a **private** repo, so branch-protection lookup is unavailable without GitHub Pro/public visibility and live settings are manual state. Full setup and troubleshooting are in [`docs/CI.md`](docs/CI.md), [`docs/LINUX-RUNNER.md`](docs/LINUX-RUNNER.md), and [`docs/WINDOWS-RUNNER.md`](docs/WINDOWS-RUNNER.md).
- **rustdoc is gated** — `cargo doc --workspace --no-deps --all-features` runs with `RUSTDOCFLAGS=-D warnings` both in `cargo xtask ci` (step 4/15) and in `.github/workflows/docs.yml`. The latter also deploys to GitHub Pages on `main` (requires Settings → Pages → Source = "GitHub Actions" as a one-time setup). `docs.yml`'s `rustdoc` job is path-skipped like the others; require the aggregate `Docs` status check. Run `just doc-check` locally before pushing to catch broken doc-links early.


<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
