# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- CUSTOMIZE: When releasing, move items from [Unreleased] to a new version section.
               Format: ## [X.Y.Z] — YYYY-MM-DD
               Use Added / Changed / Deprecated / Removed / Fixed / Security headers. -->

## [Unreleased]

### Added

- `soma-auth` gained an `upstream/` module (behind the new `upstream-oauth-rmcp`
  feature) implementing the outbound `authorization_code` + PKCE flow for
  connecting to OAuth-protected upstream MCP servers: per-`(upstream, subject)`
  token storage, single-flight refresh, AEAD encryption-at-rest, and a cached
  `AuthClient` pool. It is fully self-contained — no dependency on any
  gateway/runtime crate — via a minimal local `UpstreamConfig` shape scoped to
  just the fields the OAuth runtime reads.
- `soma-auth` gained an RFC 8252 §7.1-style native-app OAuth flow (behind the
  existing `http-axum` feature): `/native/callback` and `/native/poll` routes
  let desktop/mobile clients with no loopback listener or custom URI scheme
  complete sign-in via a server-hosted callback and poll for the resulting
  code.
- `soma-auth`'s Cargo features are now split: `http-axum` gates the
  axum/tower-based HTTP middleware and OAuth route handlers, and
  `upstream-oauth-rmcp` gates the new outbound OAuth runtime. Both default off.
- `soma-auth` now accepts OAuth Client ID Metadata Documents (CIMD) at
  `/authorize` as an alternative to Dynamic Client Registration, per the MCP
  draft authorization spec. An `https://`-shaped `client_id` is fetched
  (SSRF-guarded: static URL/query/fragment validation, DNS resolution
  rejecting the whole result set if any resolved address is private,
  address-pinned no-proxy no-redirect HTTP client, post-connect peer
  re-validation against the pin, a streaming 64 KiB response cap, and
  single-flight-locked positive/negative-result caching) and its
  `redirect_uris` are filtered through the same allowlist DCR-registered
  clients are held to before being trusted — CIMD does not bypass the
  redirect-URI trust boundary DCR enforces. Advertised via
  `client_id_metadata_document_supported: true` in AS metadata. DCR is
  unchanged and remains fully supported.
- Added non-executing drop-in provider inspection: `soma providers list|lint|status
  [--dir DIR] [--json]`. Unlike `soma providers validate|inspect|test`, these never
  build or dispatch through the live `ProviderRegistry` — they only parse manifests
  on disk via `FileProviderSource::inspect()`, so they're safe to run before the
  runtime touches TS/WASM/MCP/OpenAPI handlers. See `docs/PROVIDERS.md`.
- Added `codex-app-server-client`, a standalone, fully-typed async Rust
  client for the Codex CLI's `app-server` v2 JSON-RPC protocol. Zero
  path-dependencies on any other crate in this workspace, so it can be lifted
  into another project wholesale. Protocol types are generated at build time
  from a vendored JSON Schema; regenerate after upgrading `codex` via
  `cargo xtask codex-schema regen` (staleness is detected and warned about
  automatically). Includes a bounded `EventStream` channel (notifications are
  dropped and logged on overflow, but server requests always get a fallback
  JSON-RPC error reply rather than being silently dropped), a bounded
  outbound write queue with the same no-silent-drop treatment, a line-cap
  fix so `MAX_LINE_BYTES` is enforced on both the newline-found and
  no-newline read paths, build-time schema validation that fails loudly on
  a malformed `response_type` instead of misreading it, and
  `ServerNotification::method_name()` for logging a notification's kind
  without its full (potentially sensitive) payload. See
  `crates/codex-app-server-client/README.md`.

### Changed

- `soma-auth` no longer forces a Google re-consent screen on every dynamic
  client registration attempt — `force_consent` is now only set the first
  time a gateway has never issued a refresh token, avoiding a slow
  interactive round trip that could time out impatient MCP clients on retry.
- `soma-auth`'s default auth-database directory is now `~/.soma` instead of
  the inherited `~/.lab`.

### Fixed

- `soma-auth` module size: `authorize.rs` (869 effective lines) and
  `upstream/manager.rs` (1080 effective lines) exceeded the repo's
  `xtask patterns` file-size hard limit (700). Split DCR client
  registration and redirect_uri resolution out of `authorize.rs` into new
  `registration.rs` and `redirect_uri.rs` modules, and split
  `AuthClient`/OAuth-client-config construction out of
  `upstream/manager.rs` into a new `upstream/manager/client.rs` child
  module (a second `impl` block for the same type, not a new
  abstraction). No behavior change; `authorize.rs` is now 539 effective
  lines and `upstream/manager.rs` is 664.

- `soma-auth` CIMD (Client ID Metadata Document) hardening found by
  independent multi-agent code review: `DocumentCache`'s per-URL
  single-flight lock map (`build_locks`) is now bounded and swept of idle
  locks, closing an unauthenticated memory-exhaustion vector on
  `/authorize`; cached fetch failures now preserve their original
  `CimdError` variant instead of being downgraded to a generic
  `cimd_fetch_failed`, so security-relevant `kind()` classification (e.g.
  `ssrf_blocked`) survives cache hits; the document cache's own capacity
  cap is now actually enforced under sustained fresh-entry load instead of
  only pruning already-expired entries; the post-connect peer
  re-validation now fails closed (rejects the fetch) rather than silently
  skipping verification when the underlying HTTP client can't report a
  peer address; and the SSRF IP denylist now also blocks IPv4 Class E
  (`240.0.0.0/4`) and IPv6 multicast (`ff00::/8`), matching what it already
  claimed to block.

- `soma-auth` error/log messages that referenced token TTL environment
  variables no longer hardcode the `LAB_` prefix; they now interpolate the
  configured `env_prefix` so the message matches the variable an operator
  actually needs to set.

- Restored clean-build compatibility with the dependency versions already
  pinned in `Cargo.lock`: ported schema validation to the jsonschema 0.47
  `Validator` API, hex-encoded sha2 0.11 digests explicitly, bumped
  `sse-stream` to 0.2.4 for rmcp 2.2, and installed a rustls crypto provider
  before building the rmcp streamable HTTP client transport (reqwest 0.13
  panics without one). Warm CI caches had masked all four breakages.

## [0.4.7]

### Added

- Added tag-time npm publishing for `soma-rmcp` with trusted publishing/provenance support.

### Changed

- Bumped Soma release metadata to `0.4.7` so refreshed npm discovery metadata can ship after the already-published `0.4.6`.

## [0.4.6]

### Added

- MCP provider manifests can now proxy upstream MCP servers over streamable HTTP. `meta.mcp.url`
  infers HTTP transport automatically, while existing stdio manifests continue to work.

## [0.4.5]

### Added

- Dynamic provider runtime registry with manifest-backed MCP, REST, CLI, palette, and
  generated OpenAPI surfaces, including provider capability enforcement and contract
  checks for generated provider/palette metadata.

## [0.4.4]

### Removed

- Removed the deprecated `the retired REST action-envelope route` REST action-envelope route. REST now exposes
  only direct typed `/v1/*` business routes while MCP keeps compact action dispatch behind
  its single tool surface.

## [0.4.3]

### Added

- GitHub workflow docs now cover the full workflow inventory, TOOTIE Docker
  Linux runner layout, steamy Windows runner expectations, and sccache usage
  across Linux and Windows CI builds.
- OAuth authorization responses now include the RFC 9207 `iss` parameter on both the
  success and error redirects, set to the authorization server's issuer identifier, so
  MCP clients can detect authorization-server mix-up attacks. First step toward MCP draft
  spec (2026-07-28) compatibility.
- OAuth dynamic client registration now accepts the RFC 7591 / OIDC `application_type`
  field (`web` or `native`, defaulting to `web`), validates it, and echoes it in the
  registration response. Toward MCP draft spec (2026-07-28) compatibility.
- CORS now permits the MCP protocol headers on the `/mcp` route — `Mcp-Protocol-Version`
  (2025-06-18+) plus the draft `Mcp-Method`, `Mcp-Name`, and `x-mcp-header` (SEP-2243) —
  so browser-based MCP clients clear preflight. Toward MCP draft spec (2026-07-28)
  compatibility.
- Added a `just conformance` recipe and `conformance-baseline.yml` that boot a no-auth
  loopback server and run the official MCP conformance suite
  (`@modelcontextprotocol/conformance`), gating on a known-failure baseline (fails only on
  new regressions). Current baseline: the core protocol scenarios pass; fixture and
  optional-feature scenarios are fenced as expected failures.
- Documented the MCP draft (2026-07-28) migration plan, ownership/gap analysis, schema
  provenance, and conformance workflow in `docs/specs/mcp-draft-2026-07-28-migration.md`.
- `GET /readyz` readiness probe (public): unlike `/health` (liveness), it probes the
  upstream dependency and returns `503 Service Unavailable` when it is unreachable, so
  orchestrators only route traffic once the server can serve it.
- `GET /metrics` Prometheus endpoint (public, requires the `observability` feature): the
  server installs a global recorder at startup and exposes `soma_actions_total` and
  `soma_action_duration_ms` (labelled by `surface`/`action`/`outcome`) in text
  exposition format. Returns `503` until the recorder is installed.
- `soma_service::dispatch_action(service, action, surface)` — a unified dispatch seam
  that all surfaces (MCP, REST, CLI) now route through, emitting one structured log line
  per action (`surface`, `action`, `outcome`, `elapsed_ms`; never parameters) plus metrics.
- `require_confirmation_if_destructive(action, params)` confirmation gate in
  `soma-contracts`, enforced on the MCP and REST dispatch paths (the CLI already
  gated): a `destructive` action without `"confirm": true` returns a structured
  validation error. No-op for Soma's current actions; gates any future one.
- `.gitleaks.toml` secret-scan policy with an allowlist for placeholder/fixture
  credentials, plus a `scheduled.yml` workflow (weekly cron + `workflow_dispatch`) that
  refreshes RUSTSEC advisories without a push, and a `workflow_dispatch` trigger on CI.
- A `ci-gate` aggregation job in CI: a single required status that fails if any needed job
  ended in anything other than success or skipped (point branch protection at it).
- In-process tracing-capture test harness (`soma-test-support`: `SharedBuf`,
  `SharedWriter`, `tracing_test_lock`) and a `dispatch_logging` regression test that pins
  the structured-logging contract.
- Architecture boundary tests (`tests/architecture_boundaries.rs`) that make the thin-shim
  rule executable: the MCP/CLI shims must reach the service layer, never the transport
  client or raw HTTP.
- `release-fast` Cargo profile (release opts, no LTO, many codegen units) plus `just
  build-fast` and `just sync-container` recipes for fast local container iteration.
- `serial_test` dev-dependency; the env-mutating `config_tests` are now `#[serial]` so
  they cannot race under `cargo test` (nextest already isolates them per process).

### Changed

- MCP, REST, and CLI action dispatch now flow through `dispatch_action` for uniform
  timing, structured logging, and metrics instead of calling `execute_service_action`
  directly. `execute_service_action` remains the un-instrumented core.
- Raised MSRV from 1.90 to 1.96 (`rust-version` across all crates, `msrv.yml`, and the
  docs). The `rusqlite` 0.40 update pulls `libsqlite3-sys` 0.38, whose build script uses
  `cfg_select` (stable only as of recent Rust), so 1.90 no longer compiles the workspace.

## [0.4.2] — 2026-06-19


<!-- CUSTOMIZE: Add changes here as you work. They move to a version section on release. -->

### Added

- Manifest-backed release version gate with `release/components.toml`, xtask commands, CI enforcement, and auto-tag planning.
- Cargo-generate support for the real multi-crate workspace shape, including selectable API, CLI, web, OAuth, and observability features.
- Xtask support for syncing and checking bundled editable Aurora web source from `apps/web`.

### Changed

- Moved the root package into `crates/soma` and made the repository root a virtual Cargo workspace.
- Updated Docker, docs, tests, cargo-generate, pattern checks, and release metadata for the crate-split layout.

### Fixed

- Brought `server.json` and generated OpenAPI version metadata back in sync with the crate version.

## [0.4.1] — 2026-06-01

### Changed

- Plugin `SessionStart`/`ConfigChange` hooks now call `${CLAUDE_PLUGIN_ROOT}/bin/soma setup plugin-hook` directly instead of going through the `plugin-setup.sh` shell wrapper. The env-var mapping the script performed (`CLAUDE_PLUGIN_OPTION_*` → `SOMA_*`) now lives in `apply_plugin_options()` in `src/cli/setup.rs`, applied before `Config::load()` on the plugin-hook path.

### Removed

- `plugins/soma/hooks/plugin-setup.sh` — the wrapper was a pure env-mapping middleman now handled by the binary's `setup plugin-hook` command.

## [0.4.0] — 2026-05-14

### Added

- `.github/workflows/codeql.yml` — CodeQL SAST analysis on push to main and weekly scheduled scan; results surface in the GitHub Security tab.
- `.github/workflows/cargo-deny.yml` — license compliance, duplicate dependency, advisory, and source checks via `cargo-deny`.
- `.github/workflows/msrv.yml` — compiles against the declared `rust-version` to catch MSRV regressions early.

## [0.3.0] — 2026-05-14

### Added

- `src/cli/watch.rs` — `soma watch` subcommand for live file-system monitoring.
- `plugins/soma/monitors/` — plugin monitor definitions for event-driven automation.
- `plugins/soma/gemini-extension.json` — Gemini extension manifest for multi-platform plugin distribution.
- `.github/dependabot.yml` + `.github/workflows/dependabot-auto-merge.yml` — automated dependency updates with auto-merge for minor/patch bumps.
- `scripts/asciicheck.py`, `scripts/check-blob-size.py`, `scripts/check-dependency-updates.sh`, `scripts/check-file-size.sh`, `scripts/check-runtime-current.sh`, `scripts/validate-plugin-layout.sh`, `scripts/blob-size-allowlist.txt` — repository validation and quality scripts.
- `tests/plugin_contract.rs` — plugin contract integration tests.
- `docs/PLUGINS.md` — documentation for the plugin system and distribution model.
- `plugins/README.md`, `plugins/soma/README.md`, `plugins/soma/CLAUDE.md` — plugin-level documentation and agent guidance.
- `apps/web/README.md`, `xtask/README.md`, `tests/README.md`, `scripts/README.md` — README coverage for every major directory.
- `.claude/` — Claude Code project settings for agent-assisted development.

### Changed

- `plugins/soma/hooks/plugin-setup.sh` — significant simplification; reduced from ~500 to ~50 lines by extracting reusable logic and removing duplication.
- `Justfile` — expanded with additional recipes covering plugin validation, script checks, and workflow shortcuts.
- `lefthook.yml` — pre-commit hook additions aligned with new script suite.
- `AGENTS.md`, `CLAUDE.md` — updated agent and AI tooling guidance to reflect current project structure.
- `README.md`, `docs/PATTERNS.md` — documentation refreshed for new scripts and plugin layout.

## [0.2.0] — 2026-05-14

### Changed

- Split `src/mcp.rs` into three focused modules: `src/server.rs` (`AppState`, `AuthPolicy`, `build_auth_layer`), `src/server/routes.rs` (Axum router wiring), and `src/api.rs` (REST API handlers). `src/mcp/` now contains only MCP protocol concerns (tools, schemas, prompts, server handler).
- `mcp/rmcp_server.rs` and `mcp/tools.rs` now import `AppState`/`AuthPolicy` from `crate::server` instead of `super`.
- `allowed_origins` visibility widened from `pub(super)` to `pub` to support cross-module access from `server/routes.rs`.
- Updated `src/lib.rs` and `src/main.rs` to reflect new module layout (`pub mod api`, `pub mod server`).

### Added

- `deny.toml` — `cargo-deny` configuration enforcing license allowlist, banning `openssl`/`openssl-sys`, denying yanked crates, and restricting dependency sources to crates.io and `github.com/jmagar/lab.git`. RUSTSEC-2023-0071 acknowledged with rationale.
- `apps/web/CLAUDE.md` — guidance for using the Aurora design system shadcn registry in the Next.js web app: install commands, token conventions, full component catalog, and usage rules.
- `.git/hooks/pre-commit` — enforces the no-`mod.rs` rule at commit time; blocks any staged `mod.rs` file with a clear error message.
- `docs/PATTERNS.md` updated: §1/§1a module layouts reflect new `server`/`api` structure with all `mod.rs` references removed; §5 auth section headers updated; §45 No mod.rs section now includes the git hook script; §A1/§A2 advanced patterns updated to match actual file locations.

### Removed

- `src/mcp/routes.rs` — moved to `src/server/routes.rs`.
- Several obsolete scripts: `backup.sh`, `check-runtime-current.sh`, `plugin-setup.sh`, `reset-db.sh`, `smoke-test.sh`, `test-check-runtime-current.sh`, `validate-marketplace.sh`.
- `docs/server-json-guide.md` — content superseded by `docs/MCP-REGISTRY-PUBLISH-GUIDE.md`.

## [0.1.0] — 2026-05-13

### Added

- Layered architecture: `SomaClient` (transport) → `SomaService` (business logic) → MCP/CLI shims
- Action-based dispatch: single `soma` MCP tool with `action` parameter routing
- Both transports: Streamable HTTP (`example serve`) and stdio (`soma mcp`)
- Bearer token authentication via `SOMA_MCP_TOKEN`
- Google OAuth authentication via `SOMA_MCP_AUTH_MODE=oauth` (issues RS256 JWTs)
- Loopback/no-auth mode for local development
- MCP elicitation support (`elicit_name` action, spec 2025-06-18) with graceful fallback
- MCP resources: exposes tool schema at `soma://schema/mcp-tool`
- MCP prompts: `quick_start` prompt
- CLI with `greet`, `echo`, and `status` subcommands
- Test helpers: `loopback_state()` and `bearer_state()` for credential-free integration tests
- `AuthPolicy` enum making auth choice explicit at construction time
- CORS, Host header validation, request body size limiting built-in
- `resolve_auth_policy_kind()` — refuses to bind `0.0.0.0` without auth (Pattern §27)
- `default_data_dir()` — detects container vs bare-metal, returns `/data` or `~/.soma`
- `entrypoint.sh` — Docker entrypoint with permission setup and privilege drop to UID 1000
- `xtask` crate with `dist`, `ci`, `symlink-docs`, `check-env` commands
- `.config/nextest.toml` — nextest configuration with `default` and `ci` profiles
- `taplo.toml` — TOML formatter configuration
- `lefthook.yml` — minimal pre-commit hooks (diff_check, toml_fmt, env_guard)
- `.github/workflows/ci.yml` — CI: fmt, clippy, nextest, taplo, audit, gitleaks
- `.github/workflows/docker-publish.yml` — multi-platform Docker build + Trivy scan
- `.github/workflows/release.yml` — release binaries for linux/amd64 and linux/arm64
- `config.soma.toml` — fully annotated config sample
- `.env.example` — documented secrets sample
- `CHANGELOG.md` following Keep a Changelog format
- Workspace structure: root crate + `xtask/` member
- `symlink-docs` and `symlink-docs-inline` Justfile recipes
