# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- CUSTOMIZE: When releasing, move items from [Unreleased] to a new version section.
               Format: ## [X.Y.Z] — YYYY-MM-DD
               Use Added / Changed / Deprecated / Removed / Fixed / Security headers. -->

## [Unreleased]

### Added

- Add `crates/shared/http-server` (`soma-http-server`, layer `shared`), plan
  section 3.12's crate for reusable Axum server plumbing: listener binding
  and the `axum::serve` run loop (`server.rs`), a graceful-shutdown signal
  future (`shutdown.rs`), request-ID/tracing/timeout/body-limit/CORS
  middleware constructors (`middleware/`), generic liveness/readiness route
  wiring on top of `soma-http-api`'s probe DTOs (`health.rs`), and a generic
  not-found/method-not-allowed rejection envelope (`rejection.rs`). `apps/soma`
  now delegates its `serve_http_mcp` bind/serve/shutdown loop, its request
  tracing and body-limit layers, and its `/*` fallback to this crate instead
  of hand-rolling them, and its CORS builder wraps the crate's generic
  `cors_layer` with Soma's own origin/header policy; `apps/soma` no longer
  depends on `tower-http` directly. Acceptance: a fake Axum router with no
  Soma types anywhere in it is bound, served, and gracefully shut down
  end-to-end through the crate's `bind`/`serve`/`serve_with_shutdown`
  (`server_tests.rs`).
- Add `crates/soma/config` (`soma-config`, layer `product-support`), plan
  section 3.18's dedicated crate for Soma's own configuration/environment
  loading. Moves `Config`/`SomaConfig`/`McpConfig`/`AuthConfig`/`RuntimeMode`/
  `AuthMode`/`default_data_dir`/`load_dotenv` (`config.rs`) and the canonical
  env-var registry (`env_registry.rs`) out of `soma-contracts` verbatim,
  including their test suites.
- Add `crates/shared/http-api` (`soma-http-api`, layer `shared`), plan
  section 3.11's crate for reusable HTTP API surface mechanics: a generic
  JSON error envelope (`response.rs`, `problem.rs`), a generic
  "parse-JSON-body-or-default" helper (`json.rs`), liveness/readiness probe
  DTOs and response builders (`probe.rs`), a generic route-inventory shape
  and capabilities-response builder (`route_inventory.rs`), and pagination
  query/response DTOs (`pagination.rs`, not yet consumed — no current Soma
  route needs pagination, declared per the plan's suggested layout for the
  first one that does). `soma-api` now delegates to these helpers instead of
  keeping duplicate copies (`responses.rs`, `gateway.rs`'s formerly
  hand-rolled JSON-rejection handling, `probes.rs`, `route_inventory.rs`,
  `api.rs`'s `json_body_or_empty`). `cargo tree -p soma-http-api
  --all-features` resolves to external crates only (axum/serde/serde_json) —
  no `soma-*` dependency — matching the plan's shared-layer contract.
- Split `crates/soma/contracts` by ownership (plan section 6.2 "From
  soma-contracts", PR 13 "Split soma-contracts"): `actions.rs`
  (`SomaAction`, `ACTION_SPECS`, `ActionSpec`/`ParamSpec`/`CliSpec`, scope
  constants, `ActionError`/`ActionValidationError`), `errors.rs`
  (`ToolError`/`ServiceErrorKind`), `scopes.rs` (`ADMIN_SCOPE`), and
  `provider_validation.rs`'s Soma-specific CLI-reserved-command policy move
  into `soma-domain`, together with their test suites — placed in
  `soma-domain` rather than `soma-application` because `soma-service` (a
  dependency of `soma-application` during the PR 12 strangler migration)
  also builds its static-Rust provider catalog directly from these types;
  putting them in `soma-application` would create an
  `application` ↔ `service` dependency cycle, while every consumer
  (application, service, api, cli, mcp, integrations, runtime, apps/soma)
  can already depend on `soma-domain` without one. `token_limit.rs`
  (`MAX_RESPONSE_BYTES`, `truncate_if_needed`) moves into `soma-domain` for
  the same reason, deviating from the plan's literal "product response
  policy → soma-application" assignment (`soma-service`'s provider registry
  and `soma-mcp`'s response paging both read `MAX_RESPONSE_BYTES` and
  neither can depend on `soma-application`). `config.rs`/`env_registry.rs`
  move into the new `soma-config` crate. `soma-contracts` becomes a
  deprecated re-export facade for one migration window (every module still
  resolves at its old `soma_contracts::*` path via `pub use`) with a small
  smoke test per module confirming the re-export still resolves; PR 19
  deletes the crate. `soma-application` drops its `soma-contracts`
  dependency entirely — it now imports `soma_provider_core::{ProviderPrompt,
  ProviderResource}`, `soma_domain::scopes::{READ_SCOPE, WRITE_SCOPE}`, and
  `soma_domain::token_limit::MAX_RESPONSE_BYTES` directly — retiring the
  `application → contracts` `TEMPORARY_EXCEPTIONS` entry in
  `xtask/src/architecture.rs`. `soma-client` similarly drops `soma-contracts`
  in favor of a direct `soma-config` dependency (its only use of the facade
  was `SomaConfig`). `xtask/src/architecture_graph.rs` maps
  `crates/soma/config` to the `product-support` layer alongside
  `soma-client`. Fixed several xtask/doc-generation checks that text-scanned
  the old hardcoded `crates/soma/contracts/src/actions.rs` /
  `crates/soma/contracts/src/config.rs` paths (`xtask/src/patterns/actions.rs`,
  `xtask/src/patterns/checks.rs`, `xtask/src/scripts_lane_d.rs`,
  `scripts/generate-docs.py`, `apps/soma/tests/soma_invariants.rs`) to point
  at the new canonical locations, and regenerated the derived docs
  (`docs/ENV.md`, `docs/MCP_SCHEMA.md`, `docs/generated/openapi.json`,
  `docs/generated/plugin-settings.md`) — presentation/citation-only diffs,
  no action/schema/route content changed. While validating the
  `contract-audit` gate, also regenerated `docs/generated/palette-manifest.json`
  and `docs/generated/provider-surfaces.json` (plus their downstream
  `plugin.json`/marketplace/skill artifacts). This is a real, substantive
  schema change to the committed JSON — new top-level fields
  (`schema_version`, `title`, `publisher`, `security_policy`, `website`,
  `provider_fingerprint`, a restructured `mcp_server` block, a new
  `surfaces` block) — not mere key-ordering. It is still unrelated to this
  split, though: `xtask/src/generated_surfaces.rs`'s emitted schema already
  gained every one of these fields back in `df11915` ("chore: harden soma
  metadata validation"), a commit already on `main` well before this
  branch existed. `docs/generated/plugin.json` and
  `docs/generated/provider-surfaces.json` were simply never regenerated and
  committed against that schema afterward, so `main`'s checked-in copies
  have been stale relative to `main`'s own generator this whole time.
  Bringing them current is unrelated to the contracts split, but it is not
  presentation-only either — flagged here in case a schema consumer expects
  the old shape. Included as a minimal drive-by fix since the stale files
  otherwise fail the `contract-audit` gate this PR must pass.
- Add `crates/soma/client` (`soma-client`, layer `product-support`), plan
  section 3.19's dedicated crate for the concrete outbound HTTP transport to
  a deployed `soma serve` REST API. Moves `SomaClient` (`soma.rs` →
  `client.rs`, plus its sidecar tests) out of `soma-service`; `soma-service`
  now re-exports `SomaClient` from `soma-client` behind
  `#[deprecated(note = "use soma_client::SomaClient")]` for one migration
  window (plan PR 12's compatibility stage). All non-test production
  consumers (`apps/soma`, `xtask`) and every in-repo test import
  `soma_client::SomaClient` directly rather than the deprecated path, so
  `cargo clippy -D warnings` stays clean. `soma-service`'s own `client` and
  `observability` Cargo features now forward to `soma-client`'s identically
  named features so the existing bare-MCP-profile feature-unification
  contract (`soma-service` pulls in neither `client` nor `observability`,
  and `soma-observability` never appears in that graph) is unchanged. This
  is a partial slice of plan PR 12 ("split `soma-service`"): the remaining
  moves — business workflows into `soma-application`, invariant rules into
  `soma-domain`, the provider registry/capabilities/concrete providers into
  `soma-provider-core`/`soma-provider-adapters`/`soma-integrations`, and
  retiring the `soma-application` → `soma-service` architecture exception —
  are deferred to a follow-up slice; see the PR body for the itemized
  rationale (the provider registry still depends on `soma-contracts`, which
  the shared-layer rule blocks from moving into `crates/shared/*` until
  PR 13 splits `soma-contracts`).
- Add `crates/soma/integrations` (`soma-integrations`, layer
  `product-integration`), the product-adapter crate connecting
  `soma-application`'s transport-neutral ports to Soma's shared engines (plan
  section 3.20). Moves `apps/soma`'s temporary `GatewayPort` implementation
  (`gateway.rs`), gateway-to-auth OAuth bridge (`gateway_auth.rs`, `oauth`
  feature), and Soma's product auth default mapping (`auth.rs`, `auth`
  feature) out of `apps/soma`, which now only constructs these adapters. Adds
  a new `CodeModePort` adapter (`codemode.rs`) delegating to
  `soma_codemode::execute::execute_inline` — the port existed but had no
  product implementation before this crate. `OpenApiPort` still has no
  adapter: `OpenApiExecuteRequest` has no spec/label field and no
  `soma_openapi::registry::OpenApiRegistry` is constructed anywhere in the
  runtime, so a real adapter would invent an unspecified wire shape rather
  than move existing, tested behavior — left for a focused follow-up. The
  product-specific providers PR10 left in `soma-service` (`static_rust.rs`,
  `remote.rs`, `resource_files.rs`/`resource_uri.rs`) still depend on
  `SomaService` and `soma-service`'s local `Provider`/`ProviderCall` traits,
  neither of which are in `soma-integrations`'s declared dependency shape;
  moving them stays PR12's job (`soma-service` split), as PR10's own
  changelog entry already noted.
- Add `crates/shared/provider-adapters` (`soma-provider-adapters`), a
  feature-gated, product-neutral crate of reusable provider implementations
  (static-echo, ai-sdk, python, wasm, openapi, and a thin upstream-MCP/gateway
  projection adapter), plus a generic `manifest_file::build_provider` kind
  dispatcher. `soma-service`'s drop-in provider loader now builds these kinds
  through the shared crate (wrapped by a new `provider_registry::SharedAdapter`)
  instead of implementing them itself. Product-specific providers (Soma's
  built-in actions provider, the remote-catalog provider that calls
  `SomaService`) and the directory-scanning/Soma-CLI-policy orchestrator
  around the dispatcher stay in `soma-service` pending `crates/soma/integrations`
  (PR11). See the PR10 deviation notes for why the OpenAPI and upstream-MCP
  adapters were not fully delegated to `soma-openapi`/`soma-mcp-client`.
- Add `soma-tauri-shell`, a reusable, product-neutral Tauri desktop shell
  crate (window show/hide/resize/center, tray setup, global shortcut parse
  and rebind, blur-dismiss state and window-lifecycle helpers, atomic
  app-data JSON persistence, and Tauri command result/error helpers), and
  `soma-palette`, Soma's Palette product surface crate owning
  `/v1/palette/{catalog,search,schema,execute}` routes, Palette DTOs shared
  by the HTTP server and desktop app, the `ToolSpec` Palette-overlay to
  launcher-action mapping, launcher execution/auth policy, and Palette route
  OpenAPI metadata. `apps/palette/src-tauri` stays an app-local Tauri
  package (not a root workspace member) and now path-depends on
  `soma-tauri-shell` for its window/tray/shortcut/persistence mechanics.
- Add `soma-domain` product values and a transport-neutral `soma-application`
  facade over the legacy service/provider registry, with abstract gateway,
  Code Mode, and OpenAPI ports for incremental surface migration.
- Add an `rmcp-traces` platform crate targeting `rmcp 2.2.0` with bounded request trace metadata parsing and redacted Soma MCP trace summaries.
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
- Added Markdown-file-as-MCP-prompt support: dropping a `.md` file into the
  provider directory exposes it as an MCP prompt (file stem → prompt name,
  first `# Heading` → description, full file body → prompt template).
  `README.md` is never treated as a prompt. See `docs/PROVIDERS.md`.
- Added a structured `providers/{tools,prompts,resources}/` directory layout
  alongside root-level file loading. `tools/` and `prompts/` reuse the
  existing root-level file-type rules; `resources/` is new — any file
  (recursive) becomes an MCP resource, with static files served directly and
  `.ts` files dispatched as dynamic resource readers (parameterized/catch-all
  path templates, e.g. `service/[name].ts` → `soma://resources/service/{name}`)
  through the same sandboxed Node sidecar `ai-sdk` tool providers use.
  Enforces a path-traversal trust boundary (symlinks cannot escape the
  provider root) and `resource.scope` enforcement matching `tool.scope`.
  `resources/list`, `resources/templates/list`, and `resources/read` are
  wired into the live MCP surface for the first time. A directory refresh
  failure now keeps the last valid snapshot active instead of failing every
  provider's requests. See `docs/PROVIDERS.md` and
  `docs/contracts/drop-in-provider-layout.md`.
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
  `crates/shared/codex-app-server-client/README.md`.
- Added `soma-cli-core`, a reusable CLI plumbing crate extracted from
  `soma-cli`: common flag-scanning primitives, output-format selection,
  JSON rendering, confirmation I/O, and terminal/color capability policy
  (including the Aurora CLI token palette as reusable shared defaults).
  `soma-cli`'s argument-scanning helpers, destructive-confirmation prompt,
  JSON output rendering (`lib.rs` and `doctor.rs`), and `doctor` color
  output now delegate to it with no output change. See
  `crates/shared/cli-core/README.md`.

### Changed

- Centralize all internal crate paths and the exact `rmcp = "=2.2.0"` pin in a
  root `[workspace.dependencies]` table; member manifests now inherit them via
  `workspace = true` instead of duplicating relative paths and the rmcp pin
  across manifests. Behavior-preserving: dependency resolution and feature
  unification are unchanged.
- Store one `SomaApplication` facade in the process-wide `SomaRuntime` and keep
  legacy service, provider-registry, and gateway engines private behind narrow
  application/runtime interfaces shared by CLI, stdio, and HTTP surfaces.
- Route CLI product actions through `SomaApplication`; `soma-cli` now owns only
  parsing, confirmation I/O, rendering, and error presentation while app
  composition selects local or remote provider infrastructure.
- Route REST actions, dynamic route lookup, provider inspection, readiness,
  OpenAPI snapshots, and gateway operations through `SomaApplication`;
  `soma-api` now depends only on product application/domain contracts and HTTP
  types rather than runtime, service, provider-registry, or gateway engines.
- Route MCP tools, prompts, resources, protected gateway proxying, auth
  principals, and trace context through `SomaApplication`; `soma-mcp` no
  longer depends directly on runtime, service, or gateway engines, while
  preserving structured error, discovery, scope, and remote-error privacy
  contracts.
- Finished the MCP role-crate split (PR 14): moved the remaining generic
  inbound mechanics out of `soma-mcp` into `soma-mcp-server` — response-page
  store (already there), MCP conformance-suite fixtures, `rmcp::model::Tool`
  JSON/descriptor conversion, tool-error result shaping and the generic
  "unknown tool" protocol error, trace metadata extraction integrating
  `rmcp-traces`, and the Streamable HTTP allowed-host/origin computation and
  transport builders (new `http` feature). `soma-mcp` now only supplies Soma
  tool schemas, prompts/resources, scope mapping, and application-request
  translation; `crates/soma/mcp/src/{rmcp_server,transport,protocol_errors,gateway_proxy}.rs`
  delegate to the role crate instead of duplicating it. `soma-mcp-proxy`
  gained `rmcp_tool_from_route`/`rmcp_resource_from_route`/
  `rmcp_prompt_from_route` (built on `soma-mcp-server`, closing the
  `soma-mcp-proxy -> soma-mcp-server` edge from section 3.7 of the refactor
  plan), and `soma-gateway` gained `GatewayManager::rmcp_{tool,resource,prompt}_routes[_for_subject]`
  built the same way, closing the `soma-gateway -> soma-mcp-server` edge and
  replacing gateway's unused direct `rmcp` "server" feature request. A fake
  unrelated `ServerHandler` and a fake unrelated gateway now exercise these
  role crates end to end with no Soma product crate on their dependency
  graph (`crates/shared/mcp/server/tests/fake_server.rs`,
  `crates/shared/mcp/proxy/tests/fake_gateway.rs`).

- `soma-auth` no longer forces a Google re-consent screen on every dynamic
  client registration attempt — `force_consent` is now only set the first
  time a gateway has never issued a refresh token, avoiding a slow
  interactive round trip that could time out impatient MCP clients on retry.
- `soma-auth`'s default auth-database directory is now `~/.soma` instead of
  the inherited `~/.lab`.

### Fixed

- PR17 review fix: `soma-palette` duplicated `soma-api`'s
  `ApplicationError.code` → `StatusCode` mapping verbatim instead of sharing
  it through `soma-http-api` (both crates are `product-surface` and must not
  depend on one another); moved the mapping to
  `soma_http_api::response::application_error_status` and had both surfaces
  delegate to it. `apps/palette/src-tauri/src/labby_bridge.rs` now
  path-depends on `soma-palette` and consumes its `dto::LauncherExecuteRequest`
  and `openapi::{CATALOG_PATH, SCHEMA_PATH, EXECUTE_PATH}` instead of
  redefining the request shape and hardcoding the `/v1/palette/*` path
  strings, per plan section 6.2's move instruction for that file. Removed
  `RegistrySnapshot::cached_palette_manifest` from `soma-service`'s provider
  registry — a pre-Palette-overlay placeholder manifest that PR 17's real
  `soma_palette::catalog::catalog_response()` (backed by `ToolSpec` Palette
  overlays) superseded; it was constructed on every registry build but read
  nowhere in the workspace.
- PR13 review fix: 9 of the 11 crates touched by the `soma-contracts` split
  (`soma-api`, `soma-cli`, `soma-mcp`, `soma-integrations`, `soma-runtime`,
  `soma-service`, `soma-test-support`, `apps/soma`, `xtask`) still declared
  `soma-contracts = { workspace = true }` and imported `soma_contracts::*`
  throughout `src/`/`tests/`, so PR 13's stated acceptance criterion ("No
  production crate depends on `soma-contracts`") was unmet even though
  `soma-application` and `soma-client` had already migrated. Repointed every
  remaining `soma_contracts::actions`/`config`/`env_registry`/`errors`/
  `provider_validation`/`providers`/`scopes`/`token_limit` import to its real
  home (`soma_domain`, `soma_config`, or `soma_provider_core`) across ~50
  files, and swapped each crate's `soma-contracts` `Cargo.toml` dependency
  for the specific `soma-domain`/`soma-config`/`soma-provider-core` entries
  its code actually uses. Only `crates/soma/contracts` itself (the facade,
  self-contained) still depends on the split crates going forward.
  `xtask/src/architecture.rs`'s `check_layer_edge()` only forbade
  `ProductDomain`/`ProductApplication` from depending outward to `Legacy`,
  so `cargo xtask check-architecture` kept reporting a clean pass throughout
  — it never actually enforced this PR's acceptance bar, and couldn't
  simply forbid the whole `Legacy` layer either, since `soma-service`
  shares that layer and is still a legitimate strangler-pattern dependency
  for several surfaces. Added a dedicated `DEPRECATED_CONTRACTS_FACADE_PATH`
  check that names `crates/soma/contracts` explicitly: any edge into it now
  fails the gate, with a new `any_layer_depending_on_deprecated_contracts_facade_fails`
  regression test covering surface/integration/runtime/app/legacy callers.
  Also fixed a stale `crates/soma/cli/src/lib.rs` comment referencing
  `soma_contracts::provider_validation` (moved to `soma_domain::provider_validation`
  by this same split) and corrected a `CHANGELOG.md` entry that
  mischaracterized the regenerated `docs/generated/plugin.json`/
  `provider-surfaces.json` diff as "key-ordering/fingerprint
  non-determinism" — it is a real schema change (new `schema_version`,
  `publisher`, `security_policy`, `website`, `provider_fingerprint`,
  restructured `mcp_server`/`surfaces` blocks), just one whose generator
  code (`xtask/src/generated_surfaces.rs`) already landed on `main` via
  `df11915` ("chore: harden soma metadata validation") well before this
  branch existed — the committed JSON was simply never regenerated against
  it until this PR's `contract-audit` gate forced the catch-up, so the
  drift is real but still unrelated to the contracts split itself.

- PR12 review fix: the `soma-client` extraction (`soma.rs` → `client.rs`)
  left several docs and the `cargo xtask scaffold --adapt-plan` generator
  still pointing new-service authors at the deleted
  `crates/soma/service/src/soma.rs` path. Updated `docs/ARCHITECTURE.md`
  (diagram, module layout, file-map table), `docs/QUICKSTART.md`'s
  adaptation checklist, `docs/contracts/plugin-stdio-adapter.md`'s
  `upstream_refs`, `README.md`, and its duplicate in
  `packages/soma-rmcp/README.md` to point at `crates/soma/client/src/client.rs`.
  Updated `xtask/src/scaffold.rs`'s adapt-plan output string and its
  `adapt_plan_is_profile_aware_and_path_specific` test assertion to match, so
  the test no longer locks in the stale path as expected output.

- PR11 review fix: `soma-integrations::CodeModeApplicationPort` was
  implemented and unit-tested but never constructed anywhere outside its own
  tests, so `soma(action="code_mode")`-style callers still silently fell back
  to `UnavailableEnginePort` in production. `ApplicationPorts` gained
  `with_codemode()`/`with_openapi()` builders alongside the existing
  `with_gateway()`, and `apps/soma`'s `runtime_for_components` now wires
  `CodeModeApplicationPort::default()` into every runtime it builds.
  `apps/soma`'s `soma-integrations` dependency is also now optional and
  feature-gated (`mcp-stdio`, `mcp-http`, `test-support`) instead of
  unconditional, so `soma-gateway`'s `protected-routes` feature is no longer
  pulled into builds — e.g. a `cli`-only, `default-features = false` build of
  the lib crate — that never construct `ApplicationPorts` from it.

- `soma-provider-adapters::openapi` review fix: `OpenApiProvider` now
  delegates HTTP dispatch to `soma-openapi` (`http::execute_operation_for_allowlisted_host`,
  a new entry point for callers that have already restricted the target host
  through their own allowlist) instead of hand-rolling a second reqwest
  GET/POST/PUT/PATCH/DELETE executor, satisfying PR10's "no duplicate OpenAPI
  HTTP executor" acceptance criterion while preserving the tested loopback
  allowlist behavior and the absolute-operation-URL rejection. `manifest_file::build_provider`'s
  doc comment was also corrected — every `ProviderKind` (including
  `StaticRust`) is dispatched through it when its owning feature is enabled;
  none are constructed by call sites directly. `provider-adapters::gateway`'s
  duplicate upstream-MCP transport stack (`UpstreamMcpProvider` vs.
  `soma-mcp-client`'s pooled `UpstreamPool`) was assessed and intentionally
  left as a documented deviation — full migration needs `UpstreamConfig` to
  grow arbitrary-header support and reconciled `SpawnGuard`/timeout/response-shape
  semantics; tracked as its own follow-up (bead `rmcp-template-fnz0`) rather
  than folded into this fixup.

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
- Fixed `RegistrySnapshot::inspection_report` omitting `prompt.template` from
  its JSON, which meant a `SOMA_RUNTIME_MODE=remote` server's
  `RemoteCatalogProvider` always reconstructed remote Markdown provider
  prompts with `template: None` and silently dropped them from
  `prompts/list`/`prompts/get` (`servable_prompts` requires a template),
  even though the same prompts served correctly in local mode.
- Fixed three MCP resource gaps: `resources/list` advertised every declared
  `catalog().resources` entry, including ones from provider kinds that
  can't serve reads (always failing `resources/read` with `unknown_resource`)
  — now built from the same live, read-capable index `read_resource`
  consults. A static resource with `mcp: { enabled: false }` was still
  indexed and readable via MCP despite the overlay — resource disablement is
  now honored the same way tools/prompts honor theirs. Two parameterized
  resource templates whose literal segment falls in a different position
  (e.g. `foo/[id]` and `[kind]/bar`, both matching `foo/bar`) were not
  detected as ambiguous because the old check only compared identical
  segment shapes — ambiguity detection is now a proper pointwise overlap
  check within each precedence tier.
- Fixed a TOCTOU in `ProviderRegistry::read_resource`: the URI match and the
  provider clone were two separate lock acquisitions, so a concurrent
  `refresh_file_providers()` between them could invoke a newer snapshot's
  provider using scope/params matched against the older snapshot (e.g. a
  hot-swapped `resources/foo.md` -> `resources/foo.ts` letting a request
  matched against the old unscoped static resource run the new
  `soma:write`-scoped dynamic reader unchecked). Both are now fetched from a
  single lock acquisition, mirroring `dispatch()`'s pattern for tools.
- Fixed static resource names being derived from just the leaf filename
  stem, so `resources/api/runbook.md` and `resources/ops/runbook.md` (two
  distinct, valid, non-colliding URIs) both derived `name == "runbook"` and
  tripped the global resource-name uniqueness check, failing the whole
  directory's refresh. Names now use the same full-path-derived name the
  provider ID already used.
- Fixed `soma providers lint`/`inspect` never checking dynamic `.ts`
  resource readers for template ambiguity — `dynamic_resource_templates()`
  isn't part of `catalog()`, so two colliding readers (e.g.
  `resources/service/[name].ts` and `resources/service/[id].ts`) both
  reported as `Loaded` even though the live registry rejects the pair and
  keeps the previous snapshot at real construction time.
- Fixed two Windows-only breakages in the structured resources feature's
  own test suite (found via Windows CI after the fact, not by design):
  a test simulating a colliding drop-in file used a case-only filename
  variant (`Runbook.md` after `runbook.md`), which is the same path on
  case-insensitive filesystems (NTFS, APFS by default) and silently
  overwrote the original instead of creating a genuine second file; and
  `ProviderFileInspection.file_name` for a nested resource file rendered
  with the platform's native path separator (`\` on Windows) instead of
  the `/` its sibling `uri_template` field always uses, so a resource
  under a subdirectory reported a `file_name` that looked nothing like
  its own URI on Windows.

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
