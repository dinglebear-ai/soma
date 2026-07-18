# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- CUSTOMIZE: When releasing, move items from [Unreleased] to a new version section.
               Format: ## [X.Y.Z] — YYYY-MM-DD
               Use Added / Changed / Deprecated / Removed / Fixed / Security headers. -->

## [Unreleased]

### Added

- Add `crates/shared/self-update` as a standalone, transport-neutral binary
  update transaction with bounded streaming SHA-256 verification, timed exact-
  version validation, Unix atomic replacement, and durable health confirmation
  and rollback. Explicit crash phases make restart recovery idempotent, validator
  timeouts terminate Unix process groups and Windows Job Objects, and rollback
  state is identity-, owner-, and digest-checked. Executable leaf symlinks are
  rejected consistently, test failpoints are updater-scoped, and a deterministic
  lock-protected marker temporary is reclaimed after crashes. The largest
  reachable marker phase and attempt count are size-capped before mutation,
  generated backup identities cannot collide with
  transaction state, validation cancellation kills the full process tree, and
  transport adapters can validate every redirect and final response URL. The
  crate has no internal workspace dependencies; this change
  does not enable self-update behavior in the Soma runtime or integrate Cortex.
- Restructured `apps/soma` (plan section 3.1, PR 18) into a composition-only
  layout: `bootstrap.rs` builds the concrete dependency graph (config, the
  transport client, provider registries, gateway/Code Mode adapters,
  `SomaApplication`, `SomaRuntime`); `invocation.rs` classifies `argv` into an
  execution `Mode` (help/version/serve/stdio/cli); `local.rs` runs one-shot
  CLI commands against `Arc<SomaApplication>`; `http.rs` merges the MCP
  Streamable HTTP transport, REST API, Palette product API, OAuth discovery,
  Prometheus metrics, and the web UI fallback into one router and serves it;
  `stdio.rs` starts the product MCP adapter over stdio; `shutdown.rs` owns the
  process shutdown signal. `bin/soma.rs` is now a two-line process entry point
  that forwards `argv` to the new `soma::run` library entrypoint — mode
  selection, engine construction, and router/lifecycle composition all moved
  out of the binary and into the library crate. `http.rs` also wires
  `soma-palette`'s `/v1/palette/*` router into the composed HTTP router for
  the first time (previously built but unmounted). Replaces `runtime.rs`,
  `routes.rs`, and `application_ports.rs`. Behavior is
  unchanged: the full pre-existing `apps/soma` test suite (unit, integration,
  and architecture-boundary tests) passes unmodified in substance, with only
  file-path references updated to match the new module names.
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
- `codex-app-server-client`'s optional `rest` feature became liftable and
  operable end-to-end, without breaking the crate's zero-workspace-path-dependency
  rule (no new crates entered the dependency graph — `futures-core`,
  `tower-layer`, and `tower-service` were already transitive `axum` deps):
  - **OpenAPI**: `rest::openapi_spec()` returns an OpenAPI 3.1.0 document for
    the whole REST surface, checked in at
    `crates/shared/codex-app-server-client/openapi.json` (13 routes, 19
    schemas, `bearerAuth` scheme) so downstream clients can be generated
    without building the crate. A test enforces spec/file parity, and a
    route-coverage test probes the live router so a documented-but-unmounted
    route fails the build.
  - **Runnable binary**: `codex-app-server-rest --host --port --mode
    text-turn|trusted-bridge|health-only [--token]`, with
    `CODEX_APP_SERVER_REST_*` env fallbacks. It refuses to start on a
    non-loopback bind in `trusted-bridge` mode without a token, and prints its
    effective configuration (never the token) on startup.
  - **Bearer auth**: `rest::bearer_auth(token)` is a batteries-included
    `tower` layer — `rest::trusted_bridge_router().layer(rest::bearer_auth(token))`
    — with constant-time token comparison and a configurable health-route
    exemption (`/v1/compatibility` is never exempt).
  - **SSE**: `GET /v1/sessions/{sessionId}/events/stream` streams the same
    payloads as the long-poll `.../events` route as Server-Sent Events. A
    session still allows only one event consumer; a second reader of either
    kind gets `409 Conflict`. The stream yields to the executor on a bounded
    number of consecutive synchronous backend polls, and `?timeoutMs=` is
    clamped up to `RestLimits::min_stream_poll_timeout` (250ms) on this route
    only — without either, a backend that resolves `poll_event` synchronously
    (legal for the public `RestBackend` trait, and what a buffered event looks
    like) let one request loop the runtime without bound. The long-poll route
    keeps accepting `timeoutMs=0` verbatim: there it means "only if an event
    is already waiting", and each repeat is paced by an HTTP round trip.
  - **Operational knobs**: every `RestLimits` field (session TTL, max
    sessions, concurrency caps, response byte cap, text-turn timeout, event
    buffer size, SSE keep-alive, ...) gained a documented default and a
    `CODEX_APP_SERVER_REST_*` override via `RestLimits::from_env()` /
    `try_from_env()`. A malformed value is a hard error, never a silent
    fallback. Event buffer size reaches the real per-session channel via the
    new `SessionOptions::with_events_capacity` and
    `CodexAppServerClient::{spawn,connect_streams,connect_unix}_with_events_capacity`,
    so the REST limit configures it without misrepresenting the underlying
    constant (now `DEFAULT_EVENTS_CHANNEL_CAPACITY`) as REST-specific.
  - **Safety examples**: `rest_loopback_dev`, `rest_bearer_auth`,
    `rest_trusted_gateway`, and `rest_admin_unsafe` document the four
    deployment postures and what each does and does not protect.
  - **TypeScript client**: generated from `openapi.json` under
    `crates/shared/codex-app-server-client/clients/typescript/`, kept in sync
    by `cargo xtask check-ts-client [--write|--check]` (which skips cleanly
    when `node`/`pnpm` aren't installed). Building it proved the spec is
    consumable by a real third-party generator, and immediately caught a bug
    in it: `RestEventResponse`'s `discriminator.mapping` pointed at four
    component schemas that were only ever built inline in the `oneOf` array
    and never registered, which every spec-compliant generator rejects. The
    four variants are now real named schemas — so generated clients get named
    per-variant types rather than an anonymous union — and a new
    `every_schema_ref_resolves_to_a_real_component` test fails the build if
    any `$ref` or `discriminator.mapping` target ever dangles again.
- Add `cargo xtask codex-schema drift [--dir <dump>] [--json] [--strict]`,
  which diffs the vendored `codex-app-server-client` protocol schema against
  the installed `codex` CLI's actual app-server surface and reports added,
  removed, and changed methods per section. A scheduled
  `codex-schema-drift-monitor` workflow opens/updates a tracking issue on
  drift. A missing `codex` binary is always a graceful skip, never a failure.
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
- `codex-app-server-client` REST operational hardening:
  - The `codex-app-server-rest` binary now shuts down gracefully on `SIGTERM`
    (unix) and `ctrl-c`, draining in-flight requests instead of dropping active
    sessions and orphaning their `codex app-server` children.
  - `RestLimits::max_request_body_bytes` (env
    `CODEX_APP_SERVER_REST_MAX_REQUEST_BODY_BYTES`, default 2 MiB) caps request
    bodies on every route via axum's `DefaultBodyLimit`, replacing axum's silent
    2 MiB default and closing the input-side gap in the "every limit documented
    and overridable" contract. An oversized body is rejected with `413`
    (distinct from a malformed-JSON `400`); every request-body route documents
    it, guarded by `every_route_with_a_request_body_documents_413`.

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
- The `codex-app-server-rest` binary runs on tokio's multi-threaded runtime
  (was `current_thread`, copied unexamined from the example): it can hold up to
  `max_sessions` concurrent sessions, each driving a `codex` child, and a
  single-threaded runtime let one busy connection starve the rest. The
  single-session `examples/rest_*.rs` stay `current_thread`. The
  `rt-multi-thread` and `signal` tokio features are pulled in only by the
  `rest` feature, so a library-only consumer does not pay for them.

### Removed

- Deleted `crates/soma/service` and `crates/soma/contracts` (plan PR 19,
  "Delete legacy facades and update ecosystem artifacts"), the two legacy
  strangler-pattern crates every prior slice (PR 4-13) migrated surfaces off
  of. `crates/soma/contracts` was already a pure deprecated re-export facade
  (PR 13 had moved its real content to `soma-domain`, `soma-config`, and
  `soma-provider-core`), so deleting it needed no consumer changes.
  `crates/soma/service` still owned unmigrated business logic — `SomaService`,
  the product-policy `ProviderRegistry`, `CapabilityBroker`, and the
  filesystem/remote/static-Rust drop-in providers — left over from an
  unfinished PR 12; that code moved into `soma-application`
  (`crates/soma/application/src/{service,provider_registry,capabilities,provider_errors,providers}.rs`)
  before the crate was deleted. `apps/soma`'s public `app` module keeps
  re-exporting `SomaService` from `soma-application` so the documented
  `soma::app::SomaService` path is unaffected.
- Removed both entries from `xtask/src/architecture.rs`'s
  `TEMPORARY_EXCEPTIONS` (the `soma-application -> soma-service` strangler
  edge and the `soma-runtime -> crates/shared/mcp/gateway` edge, both
  self-documented as removable once their underlying crates/composition
  settled). `cargo xtask check-architecture` now runs with zero exceptions —
  the architecture checker's rules were updated alongside the deletion:
  `soma-application` may depend on `soma-client` (a `product-support` crate;
  previously blanket-forbidden, but this is PR 12's intended permanent
  destination for the remote Soma HTTP client, not a migration artifact), and
  `soma-runtime` (`product-runtime`) joins `app` and `product-integration` as
  a legitimate application-port/concrete-engine bridge layer, since it
  intentionally bundles the initialized `SomaApplication` handle with
  `GatewayProductState` for every surface's `AppState`.

### Fixed

- PR19 review fix (second pass): fixed a stale sidecar-test comment in
  `crates/soma/application/src/service.rs` that still pointed at the deleted
  `app_tests.rs` name instead of `service_tests.rs`; corrected an
  `apps/soma/src/lib.rs` doc comment that attributed `SomaService`'s move
  into `soma-application` to PR 12 (PR 12 only extracted provider-catalog
  contracts into `soma-provider-core`; `SomaService`/`ProviderRegistry`
  itself moved in PR 19, per the plan's own execution ledger); corrected the
  plan's PR 12 ledger row to match; synced `docs/ARCHITECTURE.md` and
  `docs/PATTERNS.md`'s module-layout trees (missing `soma-provider-core` in
  `xtask`'s dependency list, missing `palette`/`cli-core`/`http-api`/
  `http-server`/`provider-adapters`/`provider-core`/`tauri-shell` rows,
  misaligned arrows, stale `last_reviewed` date). Also regenerated
  `apps/palette/src-tauri/Cargo.lock` (a separate Cargo workspace this PR's
  ecosystem-artifact sweep missed): it still resolved `soma-service` as a
  dependency of `soma-application` from before the crate was deleted.
- PR19 review fix: `protected_routes.rs` and `protected_routes_proxy.rs`
  (moved to `crates/soma/integrations` as a PR 18 review fix behind a
  `protected-http` feature) made `soma-integrations` optionally depend on
  `soma-runtime` and `soma-mcp`, inverting plan section 3.20's target
  dependency shape (`soma-integrations` depends on application ports and
  concrete shared engines only — auth, observability, client,
  provider-adapters, gateway, codemode, openapi — never the runtime or
  surface layers built on top of it) and contradicting this crate's own
  `gateway.rs`, whose comment explicitly limits the crate's dependency
  shape to `soma-application`, `soma-domain`, and `soma-gateway` for exactly
  this reason. Moved both modules again, this time to `crates/soma/runtime`
  behind that crate's existing `protected-routes` feature (previously used
  only to forward `soma-gateway/protected-routes`; `AppState` already
  exposed `resolve_protected_route`/`resolve_protected_route_metadata`/
  `protected_route_list` under it, and `soma-runtime` already owned
  `AuthPolicy`/`build_auth_layer`). `soma-runtime` now additionally depends
  on `soma-mcp` (a `product-surface` crate) under `protected-routes` alone,
  for `McpState` and the Streamable HTTP router the gateway-subset dispatch
  path nests. `soma-integrations`'s `protected-http` feature and its
  exclusive `axum`/`reqwest`/`soma-mcp`/`soma-runtime`/`tower` dependencies
  are removed entirely. `apps/soma/src/http.rs` now wires
  `soma_runtime::protected_routes::*` instead of `soma_integrations::
  protected_routes::*`; no behavior change (bodies are unmodified, only
  import paths). Also hardened `xtask/src/architecture.rs`'s
  `check_layer_edge` to fail any `product-integration -> product-runtime`
  or `product-integration -> product-surface` edge, since neither
  `check_layer_edge` nor `check_mixed_application_and_engine_edges`
  previously caught this class of inversion; added
  `product_integration_cannot_depend_on_runtime_or_surface_crates` to
  `xtask/src/architecture_tests.rs` covering both target layers.
- PR18 review fix (second pass): `apps/soma/src/invocation.rs`'s `Mode` enum
  split into `Mode::Exit(ExitAction)` / `Mode::Dispatch(DispatchMode)` so
  `lib.rs::run()` no longer needs an `unreachable!()` backstop for the
  already-handled help/version arms — illegal dispatch-of-an-exit-action is
  now unrepresentable instead of a runtime invariant. `mod invocation` (and
  `bootstrap::init_logging`/its `tracing_subscriber` import) are now gated to
  `cli` + `mcp-stdio`, their only real caller (`run()`), fixing `dead_code`
  warnings under an `mcp-http`-only *library* build (the profile the prior
  PR18 fix restored `soma::server::serve_http_mcp` for) without risking a
  double-`tracing_subscriber::init()` panic by calling `init_logging` from
  `http::serve()` instead. Added axum-harness test coverage for
  `crates/soma/integrations/src/protected_routes.rs`'s
  `authenticate_protected_route_request`/`protected_mcp_intercept` (missing
  token, malformed token, insufficient scope, admin-scope bypass, missing
  OAuth auth state, unmatched route) and
  `protected_routes_proxy.rs`'s `protected_route_upstream_target` resolver
  (backend_url vs. upstream vs. neither, upstream-not-found,
  upstream-missing-url, unsupported-transport, bearer-token-env resolution) —
  this security-critical path had zero test coverage before. Added an
  `apps/soma` architecture-boundary test
  (`apps_soma_does_not_reintroduce_protected_route_business_logic`) so the
  protected-route logic the prior fix moved out of `apps/soma` cannot silently
  reappear there. Fixed a stale `example --help` binary name in
  `apps/soma/src/local.rs`'s unknown-command message and stale
  `apps/soma::runtime::run_cli` references in `crates/soma/cli/src/lib.rs`
  comments/panic messages (both predate this PR's `runtime.rs` ->
  `bootstrap.rs`/`local.rs` split). Minor comment-accuracy fixes in
  `local_tests.rs`/`stdio_tests.rs`/`mcp_http_roundtrip.rs`, and a doc comment
  on `ProtectedMcpState`.
- PR18 review fix: `protected_routes.rs` and `protected_routes_proxy.rs`
  (bearer-token authentication, OAuth-scope authorization, gateway-subset
  dispatch, and inbound-to-upstream proxy forwarding for protected MCP
  routes — 560 of `apps/soma`'s ~1578 `src/` lines, ~35%) implemented real
  authorization rules and gateway business workflows in the composition-root
  binary crate, contradicting PR 18's own acceptance criterion (`apps/soma`
  "contains no business rules"; plan section 3.1 lists both explicitly under
  "Does not own"). Moved both modules verbatim to
  `crates/soma/integrations` (`soma-integrations`, `product-integration`
  layer — plan section 11.1's own architecture-check example names this
  crate as the destination for exactly this kind of adapter) behind a new
  `protected-http` feature, following the same "moved out of `apps/soma`,
  permanent home here" precedent as PR 11's `gateway.rs`/`gateway_auth.rs`.
  `apps/soma/src/http.rs` now wires `soma_integrations::protected_routes::*`
  instead of constructing the logic itself; no behavior change (bodies are
  unmodified, only import paths and one `pub(super)` → `pub(crate)`
  visibility changed). Also restored a public HTTP-server bootstrap entry
  point for the `mcp-http`-only build profile: pre-PR18,
  `soma::runtime::serve_http_mcp()` was reachable under the `mcp-http`
  feature alone; PR 18 made `mod http` private with its only caller
  (`soma::run`) gated on `all(feature = "cli", feature = "mcp-stdio")`,
  silently breaking a downstream fork that embeds only the HTTP server.
  `apps/soma/src/http.rs`'s `serve()` is now `pub` and re-exported as
  `soma::server::serve_http_mcp` under `mcp-http` alone, independent of
  `cli`/`mcp-stdio`.
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
- PR17 review fix (round 2): `crates/soma/palette/src/router.rs`'s
  `post_execute` hand-rolled a `400`-only `JsonRejection` handler with its
  own `{"error": ...}` body instead of delegating to
  `soma_http_api::response::json_rejection_response` (the same helper
  `soma-api` uses), losing the `413 Payload Too Large` distinction and the
  shared `ErrorBody` shape; now delegates. `soma-palette`'s
  `launcher_not_found` 404 body is now built as an `ApplicationError` value
  instead of a hand-rolled `json!` literal, so every `/v1/palette/*` error
  response shares one wire shape. Logged (previously silent) the
  `soma-tauri-shell` poisoned-shortcut-mutex fallback and the discarded
  `unmaximize`/`set_shadow`/`is_visible` window-mechanics errors. Fixed a
  stale doc comment in `soma-palette`'s `search.rs` that described ranking
  by match position instead of by which field matched. Added missing
  behavioral test coverage: `execute_launcher`'s three outcomes, all four
  `/v1/palette/*` HTTP handlers (via `tower::ServiceExt::oneshot`),
  `palette_execution_context`'s auth/scope translation, DTO wire-format
  contracts, and edge cases in `search`/`catalog`.
- PR16 review fix: `soma-cli-core`'s `terminal` module doc comment linked to
  `crate::progress`, a module removed by the prior PR 16 reconciliation
  commit (`0e0d2b3`) for having zero call sites — `cargo doc -p
  soma-cli-core` emitted an unresolved intra-doc-link warning. Dropped the
  dangling reference. Also wired `soma-cli`'s local `parse_required_value_flag`
  to delegate to `soma_cli_core::common_args::parse_required_value_flag`
  (matching the existing delegation pattern for `reject_args`/
  `parse_bool_flag`/`parse_optional_value_flag`), giving that cli-core
  function a real call site instead of only its own unit tests; made
  `ArgParseError`'s message field private with a `message()` accessor so
  every instance is built through the crate's consistent error-wording
  helper; and added `terminal`/`confirmation` regression tests for the
  `NO_COLOR`-on-a-tty and closed-stdin confirmation paths that were
  previously untested.
- PR13 review fix (second pass): the multi-agent PR review toolkit surfaced
  further issues in the `soma-http-api`/`soma-domain` split beyond the
  dependency-migration fix above. `crates/shared/http-api/src/probe.rs`'s
  `LivenessBody`/`ReadinessBody.status` fields were bare `&'static str`
  (stringly-typed, unenforced) even though each has an exhaustively known
  set of valid values; replaced with `LivenessStatus`/`ReadinessStatus`
  enums (`#[serde(rename_all = "snake_case")]`, wire-compatible — same
  `"ok"`/`"ready"`/`"not_ready"` JSON). `crates/shared/http-api/src/
  pagination.rs`'s `PageParams::clamped()` doc comment overclaimed a
  "guarantee" the type does not actually enforce (`clamped()` is opt-in;
  nothing stops an unclamped `PageParams` reaching `Page::new`); reworded to
  state the gap explicitly instead. `crates/shared/http-api/src/problem.rs`'s
  `ErrorBody` doc claimed `error` is always "a short machine-readable code,"
  but `response.rs`'s own `json_rejection_response` (pre-existing behavior,
  unchanged by this PR) puts the framework's full rejection text there
  instead; reworded the doc to describe both real usages rather than change
  the wire response shape. Added a `crates/soma/domain/src/lib.rs` crate-doc
  comment — it was the only one of the three crates this PR adds/touches
  (`soma-config`, `soma-http-api`, `soma-domain`) missing the orientation
  doc its siblings have. Added missing test coverage: `json_rejection_response`'s
  `413 Payload Too Large` branch had no dedicated unit test in the crate
  that owns it (only covered indirectly by an unrelated `apps/soma`
  integration test); added `json_rejection_response_maps_oversized_body_to_413`
  and `_maps_malformed_json_to_400` (driving real Axum extraction failures
  through a minimal router + `DefaultBodyLimit`, new `tower` dev-dependency
  on `soma-http-api`, matching the existing pattern in `json.rs`'s tests),
  plus `page_omits_total_when_unknown` for `Page`'s `total: None`
  serialization case. Fixed three stale `crates/soma/contracts/src/*.rs`
  path references in this repo's own `CLAUDE.md` module map / "how to add
  an action" instructions (now point at `crates/soma/config/src/config.rs`,
  `crates/soma/domain/src/token_limit.rs`, `crates/soma/domain/src/actions.rs`)
  — following those instructions as written would have pointed a future
  session at the deprecated re-export facade instead of the real crate. The
  same stale `crates/soma/contracts/src/{actions,config}.rs` path pointers
  were also live (not just historical/narrative) in fourteen more stable
  docs this PR's split made incorrect: `docs/ARCHITECTURE.md` (module table plus
  its "all action metadata starts in..." invariant and its `xtask` dependency
  list), `docs/CLAUDE.md`'s "env var names are authoritative in..." rule,
  `docs/AGENTS-FIRST.md`, `docs/API.md`, `docs/CONFIG.md`, `docs/AUTH.md`,
  `docs/DOCS.md`, `docs/PATTERNS.md`, `docs/SERVICE_SURFACE_SUGGESTIONS.md`,
  `docs/QUICKSTART.md`, `docs/specs/scaffold-intent-handoff.md`, `README.md`,
  `scripts/README.md`, and its duplicate `packages/soma-rmcp/README.md` —
  repointed all of them at `crates/soma/domain/src/actions.rs` /
  `crates/soma/config/src/config.rs`. (`docs/sessions/**`,
  `docs/superpowers/plans/**`, and `soma-architecture-refactor-plan-v3.md`
  are historical/ledger records per `docs/CLAUDE.md` and intentionally left
  alone.) Two functional (non-doc) staleness bugs of the same shape: `xtask/
  src/patterns/checks.rs`'s `REQUIRED_PATTERN_FILES` — the file-existence
  list backing `cargo xtask patterns`'s `docs/PATTERNS.md` conformance check
  — still listed `crates/soma/contracts/src/{actions,config}.rs`; since the
  deprecated facade files still physically exist, the check keeps passing
  today but is asserting the wrong path is canonical, and would break for
  an unrelated reason (files genuinely missing) once PR 19 deletes the
  facade unless someone remembered to fix this list first. Repointed it now,
  consistent with `action_surfaces()`'s and `config_and_auth()`'s
  already-repointed reads in the same module. `xtask/src/scaffold.rs`'s
  `cargo xtask scaffold --adapt-plan`/action-snippet generators (the same
  adapt-plan output a PR12 review fix already repointed off a deleted
  `soma.rs` path) still told new-service authors to add actions/config to
  `crates/soma/contracts/src/*.rs`; repointed to `soma-domain`/`soma-config`.

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

- PR12 review fix (round 2): `crates/soma/client/src/client.rs`'s module doc
  still said `` `SomaService` (in `soma-application`) wraps this `` — stale
  from before the extraction; `SomaService` lives in `soma-service`, not
  `soma-application`. The `client`-feature-disabled error path also still
  said `"soma-service was built without the `client` feature"`, misnaming
  the crate that actually owns the feature. Both now say `soma-client`. The
  crate-root doc in `lib.rs` overclaimed "no ... validation logic of its
  own" when `resolve_remote_rest_call`/`remote_provider_route` do resolve
  REST method/path from the provider catalog and `validate_action_path_segment`
  does validate the action segment; the doc now describes that as
  transport-shape routing rather than denying it exists. Added missing
  `soma-client` unit coverage for `ready()` (stub always-ready, upstream
  `/health` success and non-2xx failure), `call_deployed_api_method`'s
  non-success-status and invalid-JSON-body error branches,
  `remote_provider_route`'s `surfaces.rest == false` bail branch, and
  `validate_action_path_segment` (empty/`/`-containing actions, plus
  `call_rest_action` short-circuiting before any network call). Fixed a
  discarded `axum::serve` `Result` in the new
  `apps/soma/tests/mcp_http_roundtrip.rs` test harness that would have
  silently swallowed a server-task failure instead of surfacing it. Fixed
  an unrestored `SOMA_SUPPRESS_STALE_BINARY_WARNING` env var in
  `crates/soma/cli/src/cli_tests.rs`'s `run_status_command_prints_status_json`
  that could leak into other tests sharing the same test binary.

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
  tests, so any future caller of `SomaApplication::codemode_execute` (no
  MCP action, CLI command, or REST route dispatches to it yet — that wiring
  is a separate follow-up) would have silently hit `UnavailableEnginePort` in
  production instead of a real adapter. `ApplicationPorts` gained
  `with_codemode()`/`with_openapi()` builders alongside the existing
  `with_gateway()`, and `apps/soma`'s `runtime_for_components` now wires
  `CodeModeApplicationPort::default()` into every runtime it builds — proven
  by a new `apps/soma` test that calls `codemode_execute` through the real
  composition and asserts the error is no longer `engine_unavailable`.
  `apps/soma`'s `soma-integrations` dependency is also now optional and
  feature-gated (`mcp-stdio`, `mcp-http`, `test-support`) instead of
  unconditional, so `soma-gateway`'s `protected-routes` feature is no longer
  pulled into builds — e.g. a `cli`-only, `default-features = false` build of
  the lib crate — that never construct `ApplicationPorts` from it.
  `CodeModeApplicationPort::execute` also now checks `CodeModeConfig::enabled`
  before running a snippet (the wired default is disabled) and maps
  `soma-codemode`'s `ToolError` variants to distinct `PortError` codes
  instead of one generic `codemode_execution_failed`; `soma-integrations`'s
  gateway MCP-proxy error mapping now reuses `soma-gateway`'s own exhaustive
  `GatewayManagerError` → `GatewayStructuredError` classification instead of
  marking every proxy failure `retryable: true`.

- `soma-provider-adapters` PR10 second review pass: `UpstreamMcpProvider`'s
  `static_args` (a per-manifest pin, e.g. restricting a generic upstream
  tool's `action`) were applied *before* caller-supplied params and so could
  be silently overridden by a colliding caller key; merge order is now
  reversed so the pin always wins. `openapi.rs`'s `validate_base_url` now
  fails closed when a provider's `capabilities.network` grant is absent or
  disabled — previously that silently skipped the allowlist check the
  adapter's own docs describe as its SSRF defense — and its dispatch client
  now disables HTTP redirects so an allowlisted host can't hand a request off
  to a non-allowlisted address via a 3xx response. `soma-openapi`'s internal
  `execute_operation_inner` now takes a `DispatchTrust` enum instead of two
  independent booleans, making the untested/unneeded
  `enforce_ssrf && lenient_body` combination unrepresentable. The `wasm`
  feature was missing its `sidecar` feature dependency (compiled only by
  accident whenever another sidecar-owning feature was also enabled);
  `manifest_file::build_provider` returning `None` for an unbuilt provider
  kind is now a per-manifest `FileProviderLoadError` instead of an
  `unreachable!()` that would have crashed the whole server; and
  `project_gateway_action_catalog` returns `Result` instead of panicking on
  an invalid provider id. Also: capture bounded upstream stderr as private
  diagnostics on MCP stdio provider failures (previously piped to
  `Stdio::null()` and discarded), log (rather than silently swallow) upstream
  MCP session-cancel errors and invalid provider catalog timeout env values,
  and add unit coverage for `expand_env_templates`, the `static_args` pin,
  and the fail-closed network-capability/params-must-be-object/path-parameter
  behaviors that shipped undocumented-but-untested in the first PR10 pass.

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

- `codex-app-server-client`'s REST backend swept idle sessions on every single
  `session()` call — i.e. on every event poll and session call — and each sweep
  is an O(sessions) scan behind the global session lock. The sweep on that hot
  path is now throttled to at most once per second (capped by
  `idle_session_ttl`). `create_session` and `list_sessions` still sweep
  unconditionally, because their correctness depends on a fresh view: one
  reclaims a session slot before taking it, the other must not report a
  session it is about to drop.
- `cargo xtask check-test-siblings` reported "all source files have a
  `_tests.rs` sibling" while only looking at 10 of the workspace's 22 members —
  a pass that meant it had not looked. Its root list is now split into checked
  and explicitly-exempt-with-a-reason, a test fails if a member is in neither
  (so a new crate cannot be silently unchecked), and the command prints how
  many trees it checked and names the ones it skipped. Coverage widened from 10
  to 15 trees.
- `codex-app-server-client`'s OpenAPI route table could drift in one
  direction undetected: a route mounted in `routes.rs` but never added to
  `ROUTES` compiled, passed every test, and was silently absent from
  `openapi.json` and every generated client. A new test reads `routes.rs`'s
  own `.route(...)` path literals and set-compares them against the table, so
  both directions now fail loudly.
- `codex-app-server-client` module size: `src/rest/openapi.rs` (1154 effective
  lines) exceeded the `xtask patterns` file-size hard limit (700). Split into
  `openapi/{json,route_table,schemas,paths}.rs` along the document's own
  seams; `openapi.json` is byte-identical through the move, which its parity
  test proves. `xtask patterns` also now exempts checked-in generator output
  under a `src/generated/` directory (deliberately narrow, so a hand-written
  module cannot opt out by naming): splitting is not an option for a file its
  generator rewrites wholesale and a parity test guards, so the warning could
  never be actionable.
- `cargo xtask check-openapi --help` and `check-schema-docs --help` printed
  usage and then ran the command anyway. `CheckMode::parse` now returns `None`
  for `--help` so the caller stops. `check-ts-client` shares that parser
  rather than hand-rolling a third copy of the same `--write`/`--check`
  grammar, and its `--help` no longer triggers a `pnpm install`.
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
- PR15 review fix: `soma-http-server`'s `Cargo.toml` carried an unused
  `serde` dependency and a `serde_json` dependency that was only ever used
  by `#[cfg(test)]` code (moved to `[dev-dependencies]`); `ServerError` is
  now `#[non_exhaustive]` since it's a shared-crate error type multiple
  product surfaces will consume; `apps/soma`'s CORS origin-parsing now logs
  an aggregate `error` (not just per-origin `warn`s) when every configured
  origin fails to parse, since that specific outcome silently converts an
  intended allow-list into "no browser origin permitted"; corrected several
  doc comments in the new crate that overclaimed adoption in present tense
  (`request_id`/`method_not_allowed`/`health` router helpers have no
  consumer yet; the `request_id.rs` doc example contradicted `tracing.rs`'s
  own doc about what the default trace layer captures, and was excluded
  from compilation via `` ```ignore ``, so the mismatch went unnoticed);
  documented `shutdown.rs`'s known limitation where a failed signal-handler
  registration silently degrades or disables graceful shutdown; and added
  missing test coverage: a disallowed-CORS-origin case, an exact-body-size
  boundary case, a test proving an in-flight request actually drains across
  graceful shutdown rather than merely not hanging, and a regression test
  for the `apps/soma` unmatched-route 404 envelope.

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
