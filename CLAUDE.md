# rmcp-template — Claude Code instructions

## What this project is

A reusable Rust template for building MCP servers with the rmcp crate. The binary is named `example`. All stub identifiers (`Example*`, `RTEMPLATE_*`) are renamed when the template is used for a real service.

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
- The branch keeps the template/plugin skill surface available while removing
  bundled MCP server registration for environments where the MCP server is
  already connected through the Labby gateway.

## Module map

| File | Role |
|------|------|
| `crates/rtemplate-service/src/example.rs` | `ExampleClient` — HTTP/API transport stub; one method per remote operation |
| `crates/rtemplate-service/src/app.rs` | `ExampleService` — business layer; all logic lives here, never in shims |
| `crates/rtemplate-runtime/src/server.rs` | `AppState`, `AuthPolicy`, `build_auth_layer` — HTTP server state and auth policy |
| `crates/rmcp-template/src/routes.rs` | Axum router: `/mcp`, `/health`, `/status`, OAuth discovery routes |
| `crates/rtemplate-api/src/api.rs` | REST API handlers: direct `/v1/{action}` routes, `GET /health`, `GET /status` |
| `crates/rtemplate-mcp/src/lib.rs` | MCP protocol layer — re-exports from `mcp/` submodules |
| `crates/rtemplate-mcp/src/tools.rs` | MCP shim: parse JSON args → call service → return `Value` |
| `crates/rtemplate-mcp/src/schemas.rs` | Tool JSON schema derived from the service action registry |
| `crates/rtemplate-mcp/src/rmcp_server.rs` | `ServerHandler` impl: tools, resources, prompts, scope checks |
| `crates/rtemplate-mcp/src/prompts.rs` | MCP prompts (`quick_start`) |
| `crates/rtemplate-contracts/src/config.rs` | `Config`, `ExampleConfig`, `McpConfig`, `AuthConfig`, env loading |
| `crates/rtemplate-cli/src/lib.rs` | CLI shim: parse args → call service → print |
| `crates/rtemplate-cli/src/doctor.rs` | Pre-flight checks: env, connectivity, config validation |
| `crates/rtemplate-cli/src/setup.rs` | Interactive first-run / plugin setup wizard |
| `crates/rtemplate-cli/src/watch.rs` | Polls `/health` and emits state-change lines for plugin monitor |
| `crates/rtemplate-mcp/src/transport.rs` | Streamable HTTP transport wiring and session lifecycle |
| `crates/rtemplate-contracts/src/token_limit.rs` | Token budget enforcement for MCP response payloads |
| `crates/rmcp-template/src/main.rs` | Full server binary mode dispatch |
| `crates/rmcp-template/src/bin/example.rs` | Local CLI + stdio MCP binary dispatch |
| `crates/rmcp-template/src/lib.rs` | Public facade + `testing` helpers for integration tests |
| `crates/rmcp-template/tests/cli_parse.rs` | CLI argument parsing tests |
| `crates/rmcp-template/tests/tool_dispatch.rs` | MCP tool dispatch tests (service-layer, no real credentials) |

## The thin-shim rule — enforce this hard

`crates/rtemplate-mcp/src/tools.rs` and `crates/rtemplate-cli/src/lib.rs` contain **zero business logic**. They only:
1. Parse their input format (JSON args or CLI flags)
2. Call the corresponding `ExampleService` method
3. Return the result

If you find yourself computing, filtering, transforming, or validating data in `tools.rs` or `cli.rs`, stop and move it to `app.rs`.

## How to add an action

1. **`crates/rtemplate-service/src/example.rs`** — add `pub async fn your_action(&self, ...) -> Result<Value>` with the actual HTTP/API call (or stub).

2. **`crates/rtemplate-service/src/app.rs`** — add a delegating method: `pub async fn your_action(&self, ...) -> Result<Value> { self.client.your_action(...).await }`.

3. **`crates/rtemplate-service/src/actions.rs`** — add the action to `ACTION_SPECS`, including scope, transport, CLI flags, REST path, validation metadata, and the dispatch arm in `execute_native_action()`.

4. **Generated surfaces** — run the schema/OpenAPI generation checks so `crates/rtemplate-mcp/src/schemas.rs`, generated docs, and OpenAPI reflect the service registry.

5. **Shim checks** — do not add MCP/CLI/REST business logic. The existing shims should discover the action through `rtemplate_service::action_registry()`.

6. **`crates/rmcp-template/tests/tool_dispatch.rs` and REST/CLI tests** — add tests for dispatch, validation, and generated surface parity.

7. **`CHANGELOG.md`** — add an entry under `[Unreleased]` describing the new action.

For actions with parameters, extract them with `string_arg(&args, "param_name")` in `tools.rs`.

## Auth model

`AuthPolicy` is an enum with four variants:

| Variant | When | Effect |
|---------|------|--------|
| `AuthPolicy::LoopbackDev` | `no_auth=true` or host is loopback (`localhost`, `127.*`, `::1`) via `McpConfig::is_loopback()` | No auth middleware; scope checks bypassed |
| `AuthPolicy::TrustedGatewayUnscoped` | `RTEMPLATE_NOAUTH=true` on non-loopback behind an authz-enforcing gateway | No auth middleware; scope checks bypassed |
| `AuthPolicy::Mounted { auth_state: None }` | Default non-loopback | Static bearer token required |
| `AuthPolicy::Mounted { auth_state: Some(_) }` | `auth_mode = "oauth"` | Full Google OAuth + RS256 JWT issuance |

Auth is selected in `build_auth_policy()` in `main.rs`. Scopes are `example:read` and `example:write` (write satisfies read). `help` requires no scope. Unknown actions get `DENY_SCOPE`.

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RTEMPLATE_API_URL` | — | Upstream service base URL |
| `RTEMPLATE_API_KEY` | — | Upstream service API key |
| `RTEMPLATE_MCP_HOST` | `127.0.0.1` | Bind host |
| `RTEMPLATE_MCP_PORT` | `40060` | Bind port |
| `RTEMPLATE_MCP_NO_AUTH` | `false` | Disable auth (loopback only) |
| `RTEMPLATE_MCP_TOKEN` | — | Static bearer token |
| `RTEMPLATE_MCP_ALLOWED_HOSTS` | — | Extra comma-separated Host header values |
| `RTEMPLATE_MCP_ALLOWED_ORIGINS` | — | Extra comma-separated CORS origins |
| `RTEMPLATE_MCP_PUBLIC_URL` | — | Public URL for OAuth metadata endpoints |
| `RTEMPLATE_MCP_AUTH_MODE` | `bearer` | `bearer` or `oauth` |
| `RTEMPLATE_MCP_GOOGLE_CLIENT_ID` | — | Google OAuth client ID |
| `RTEMPLATE_MCP_GOOGLE_CLIENT_SECRET` | — | Google OAuth client secret |
| `RTEMPLATE_MCP_AUTH_ADMIN_EMAIL` | — | OAuth admin email |
| `RUST_LOG` | `info` | Log filter |

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
cargo build --release     # produces target/release/example
cargo test                # all tests
cargo clippy -- -D warnings  # lint (must pass)
cargo fmt                 # format

just dev                  # RTEMPLATE_MCP_HOST=127.0.0.1 RTEMPLATE_MCP_NO_AUTH=true cargo run -- serve mcp (loopback only, no auth)
just test                 # cargo test
just lint                 # cargo clippy -- -D warnings
just fmt                  # cargo fmt
just gen-token            # openssl rand -hex 32
just health               # curl http://localhost:40060/health | jq .
```

## Test helpers

`crates/rmcp-template/src/lib.rs` exports `testing::loopback_state()` and `testing::bearer_state(token)` (behind `features = ["test-support"]` or `cfg(test)`). Use these in integration tests — they build `AppState` without real credentials.

## CLI ↔ MCP action parity

Every action in the MCP tool must also be reachable from the CLI, and vice versa.
Both shims call the same `ExampleService` methods, so parity is automatic when the
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
| `service.greet(name)` | `example(action="greet")` | `rtemplate greet [--name N]` | `POST /v1/greet` |  |
| `service.echo(message)` | `example(action="echo")` | `rtemplate echo --message <msg>` | `POST /v1/echo` |  |
| `service.status()` | `example(action="status")` | `rtemplate status` | `GET /v1/status` |  |
| `built-in help` | `example(action="help")` | `rtemplate --help` | `GET /v1/help` |  |
| `MCP client interaction` | `example(action="elicit_name")` | `_MCP-only_` | _MCP-only_ | MCP-only; requires elicitation-capable client |
| `MCP elicitation wizard` | `example(action="scaffold_intent")` | `_MCP-only_` | _MCP-only_ | MCP-only; requires elicitation-capable client |
<!-- END GENERATED CLAUDE_PARITY_TABLE -->
**TEMPLATE:** Replace this table with your service's actual actions when you adapt
the template. The rule is: one row per service method, with both the MCP action name
and the CLI subcommand/flag documented.

## Plugin versioning

Plugin manifests (`.claude-plugin/plugin.json`, `.codex-plugin/plugin.json`, `gemini-extension.json`) do **not** contain a `version` field. The marketplace derives the version from the git commit SHA on every push — adding an explicit version causes every push to be treated as a new version and creates duplicate entries. Do not add `version` to any plugin manifest and do not run `scripts/bump-version.sh` targets against plugin manifests.

## Release versioning

`release/components.toml` is the source of truth for versioning. This template currently has one shipped component, `template`, covering the Rust crate/binaries, embedded web assets, Docker/runtime files, MCP registry metadata, and plugin package files. `crates/rmcp-template/Cargo.toml` package `rmcp-template` is canonical; `Cargo.lock`, `server.json`, `docs/generated/openapi.json`, and `CHANGELOG.md` must stay in parity through the manifest. Plugin manifests are listed as `json_no_version` and must remain versionless.

Use:

```bash
cargo xtask check-version-sync
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
cargo xtask release-plan --head HEAD --mode main --json
cargo xtask bump-version template patch
```

PR mode uses the merge-base of the PR branch and `origin/main`; main mode compares against the latest matching `v*` semver tag and powers `.github/workflows/auto-tag.yml`.

## Common gotchas

- **Stdio mode suppresses logs** — `main.rs` sets log level to `warn` in stdio mode so JSON-RPC is not corrupted by log lines on stdout.
- **Scope checks run in `rmcp_server.rs`**, not in `tools.rs`. `tools.rs` only dispatches.
- **`help` action is public** — `required_scope_for("help")` returns `None`. All other actions require at least `example:read`.
- **Default port is 40060** — set in `default_mcp_port()` in `config.rs`. Override with `RTEMPLATE_MCP_PORT`.
- **`elicit_name` is MCP-only** — elicitation requires a live client connection; it cannot be invoked from the CLI. This is the one intentional parity exception.
- **`watch`, `serve`, and `doctor` are CLI infrastructure** — they are not MCP actions and have no parity requirement. `watch` polls `/health` and emits state-change lines to stdout (used by the plugin monitor). `serve` starts the HTTP server. `doctor` runs pre-flight checks. None belong in the MCP parity table.
- **CI runs on self-hosted runners behind path-aware gates** — Linux jobs use `[self-hosted, tootie, rmcp-template]`, Windows native artifact checks use `[self-hosted, Windows, rmcp-template, steamy]`, and both `ci.yml` and `msrv.yml` route jobs through `cargo xtask changed-paths`. Branch protection should require the stable aggregate `CI Gate` and `MSRV Gate` statuses, not individual path-skipped jobs. This is a **private** repo, so branch-protection lookup is unavailable without GitHub Pro/public visibility and live settings are manual state. Full setup and troubleshooting are in [`docs/CI.md`](docs/CI.md), [`docs/LINUX-RUNNER.md`](docs/LINUX-RUNNER.md), and [`docs/WINDOWS-RUNNER.md`](docs/WINDOWS-RUNNER.md).


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
