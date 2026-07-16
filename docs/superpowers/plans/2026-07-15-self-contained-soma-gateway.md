# Port Self-Contained Soma Gateway

Date: 2026-07-15
Epic: `rmcp-template-0lnb`
Execution branch base observed during planning: `codex/port-palette-app`

## Goal

Build a self-contained Soma gateway port from the Labby gateway design in `/home/jmagar/workspace/lab/crates/labby-gateway`, including the live gateway-owned MCP routing surfaces: HTTP/SSE, stdio, websocket, tools, resources, prompts, protected routes, admin config, upstream OAuth, and subject-scoped routing.

`soma-gateway` must not depend on any `labby-*` crate and must not depend on Soma product/runtime/shim crates: `soma`, `soma-runtime`, `soma-service`, `soma-contracts`, `soma-mcp`, `soma-api`, or `soma-cli`.

The only allowed internal Soma dependencies are optional leaf crates when avoiding duplication would be worse:

- `soma-auth` behind the `oauth` feature.
- `soma-codemode` behind the `codemode` feature.
- `soma-openapi` behind the `openapi` feature, or via `soma-codemode` if that is the cleaner adapter path.

Code Mode and OpenAPI are intentionally feature-gated adapter seams while those crates are being ported. They are not blockers for the gateway crate becoming self-contained or for gateway-owned live MCP transport parity.

All former `labby-runtime`, `labby-primitives`, `labby-winjob`, and dev-only `labby-apis` slices used by the gateway must become local `soma-gateway` modules or test fixtures.

## Non-Negotiables

- Use sibling test files for every new/touched Rust source module.
- No new or touched Rust source/test file over 500 physical LOC.
- No `mod.rs`.
- No follow-up beads for known blockers. Fix every encountered blocker in-session.
- Preserve unrelated worktrees and protected `marketplace-no-mcp`.
- Product crates may depend on `soma-gateway`; `soma-gateway` may not depend back on product crates.
- Business logic belongs in `GatewayManager`, `UpstreamPool`, local config/security/process modules, or feature-gated adapters. Product/API/MCP/CLI surfaces parse, delegate, and return.

## Current Branch Evidence

At plan-writing time this checkout is clean on `codex/port-palette-app`, with recent commits:

```bash
git status --short --branch
git log --oneline -5
```

Observed output includes:

```text
## codex/port-palette-app...origin/codex/port-palette-app
ea2b344 feat: add Labby palette app
4f9efce soma-auth: labby-auth port, MCP spec compliance fixes, CIMD support (#131)
be9c007 docs: save session log
```

This branch already contains the auth prerequisite commit `4f9efce`:

```bash
test -d crates/soma-auth/src/upstream
git merge-base --is-ancestor 4f9efce HEAD
rg -n "upstream-oauth-rmcp|pub mod upstream" crates/soma-auth
```

The implementation still must verify `soma-auth` with tests and must not assume this is true on any different branch base.

## Worktree Setup

Do implementation in an isolated worktree, per `vibin:work-it`.

```bash
cd /home/jmagar/workspace/soma
git status --short --branch
git fetch origin
git worktree add .worktrees/soma-gateway-self-contained -b codex/soma-gateway-self-contained HEAD
cd .worktrees/soma-gateway-self-contained
bd update rmcp-template-0lnb --claim
```

If the branch already exists, inspect it and reuse only if it is the intended branch:

```bash
git worktree list
git branch --list 'codex/soma-gateway-self-contained'
```

## Phase 1: Auth Prerequisite Verification (`rmcp-template-0lnb.1`)

Verify the auth leaf crate is present on the execution branch and green.

```bash
git merge-base --is-ancestor 4f9efce HEAD
cargo test -p soma-auth --features upstream-oauth-rmcp
cargo clippy -p soma-auth --all-targets --features upstream-oauth-rmcp -- -D warnings
cargo tree -p soma-auth --features upstream-oauth-rmcp | rg 'labby-' && exit 1 || true
```

If any touched `soma-auth` file is over 500 LOC, split it before closing this phase:

```bash
git diff --name-only origin/main...HEAD -- 'crates/soma-auth/**/*.rs' \
  | xargs -r -n1 wc -l \
  | awk '$1 > 500 { bad=1; print } END { exit bad }'
```

If `soma-auth` is missing on the actual execution branch, stop gateway OAuth work and reconcile the auth branch into this worktree first. Do not port auth into `soma-gateway`.

## Phase 2: Scaffold `soma-gateway` (`rmcp-template-0lnb.2`)

Add the workspace crate and initial boundary tests before porting gateway logic.

Files:

- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/Cargo.toml`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/Cargo.lock`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/crates/soma-gateway/Cargo.toml`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/crates/soma-gateway/CLAUDE.md`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/crates/soma-gateway/AGENTS.md`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/crates/soma-gateway/GEMINI.md`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/crates/soma-gateway/src/lib.rs`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/crates/soma-gateway/src/lib_tests.rs`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/crates/soma/tests/architecture_boundaries.rs`
- `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained/xtask/src/main.rs`

Initial `crates/soma-gateway/Cargo.toml` feature shape:

```toml
[features]
default = []
oauth = ["dep:soma-auth", "soma-auth/upstream-oauth-rmcp"]
codemode = ["dep:soma-codemode"]
openapi = ["codemode", "dep:soma-openapi"]
palette = ["codemode"]
protected-routes = []
```

If `soma-codemode` or `soma-openapi` are not present yet on the branch, use inert placeholder feature names only and add tests that prove no hidden dependency is pulled. Re-verify real feature wiring when `rmcp-template-ehml` lands.

Add `crates/soma-gateway/src` to the hardcoded sibling-test roots in `xtask/src/main.rs`.

Run:

```bash
cargo xtask symlink-docs
cargo test -p soma-gateway --no-default-features
cargo tree -p soma-gateway --no-default-features -e features
cargo test -p soma --test architecture_boundaries
cargo xtask check-test-siblings
cargo xtask patterns
find crates/soma-gateway -type f -name '*.rs' -print0 \
  | xargs -0 -n1 wc -l \
  | awk '$1 > 500 { bad=1; print } END { exit bad }'
```

Boundary tests must fail if `soma-gateway` depends on `labby-*` or forbidden Soma product/runtime/shim crates.

## Phase 3: Config, DTOs, Persistence, Redaction (`rmcp-template-0lnb.3`)

Port config DTOs and persistence only. Domain behavior stays in later phases.

Files:

- `crates/soma-gateway/src/config.rs`
- `crates/soma-gateway/src/config_tests.rs`
- `crates/soma-gateway/src/config/defaults.rs`
- `crates/soma-gateway/src/config/defaults_tests.rs`
- `crates/soma-gateway/src/config/upstream.rs`
- `crates/soma-gateway/src/config/upstream_tests.rs`
- `crates/soma-gateway/src/config/protected_routes.rs`
- `crates/soma-gateway/src/config/protected_routes_tests.rs`
- `crates/soma-gateway/src/config/virtual_servers.rs`
- `crates/soma-gateway/src/config/virtual_servers_tests.rs`
- `crates/soma-gateway/src/gateway/config_store.rs`
- `crates/soma-gateway/src/gateway/config_store_tests.rs`

Tests:

- TOML round-trip and default install.
- `.soma` config/env path safety.
- Env-file writes use mode `0600` on Unix when secrets are present.
- Raw bearer values never appear in config view/list/status/log/journal fixtures.
- `bearer_token_env` rejects raw `Bearer`, JWT-looking values, `sk-*`, `ghp_*`, and other token-like values.
- Shared redaction golden corpus covers args, URLs, `Authorization`, OAuth secrets, token-looking strings, and split flags such as `--api-key secret`.

Run:

```bash
cargo test -p soma-gateway --no-default-features config
cargo test -p soma-gateway --no-default-features redaction
cargo xtask check-test-siblings
find crates/soma-gateway -type f -name '*.rs' -print0 | xargs -0 -n1 wc -l | awk '$1 > 500 { bad=1; print } END { exit bad }'
```

## Phase 4: Security Policies (`rmcp-template-0lnb.4`)

Implement local gateway security primitives.

Files:

- `crates/soma-gateway/src/security.rs`
- `crates/soma-gateway/src/security_tests.rs`
- `crates/soma-gateway/src/security/ssrf.rs`
- `crates/soma-gateway/src/security/ssrf_tests.rs`
- `crates/soma-gateway/src/security/redact.rs`
- `crates/soma-gateway/src/security/redact_tests.rs`
- `crates/soma-gateway/src/security/env.rs`
- `crates/soma-gateway/src/security/env_tests.rs`
- `crates/soma-gateway/src/net.rs`
- `crates/soma-gateway/src/net_tests.rs`

Typed policies:

```rust
pub enum OutboundPolicy {
    StrictExternal,
    AdminProtectedBackend,
}
```

`StrictExternal` covers external/OpenAPI-like fetches: no ambient proxy, redirects disabled, DNS validation, pinned address or equivalent connect control, peer/final-address recheck, and deny private/loopback/link-local/CGNAT/metadata targets unless a later adapter delegates to `soma-openapi`.

`AdminProtectedBackend` may allow operator-configured LAN/CGNAT targets, but still denies localhost, link-local, metadata services, wildcard/private-TLD surprises, redirects to denied targets, and DNS/peer mismatch.

If Lab's basename-only spawn guard is preserved, document it as an accepted residual and add tests for:

- `/tmp/x/node`
- symlinked allowed basenames
- PATH poisoning
- lowercase env names
- `LD_PRELOAD`
- split secret flags

No remote action, import, or proposed-spec path may set or weaken `disable_spawn_guard`.

Run:

```bash
cargo test -p soma-gateway --no-default-features security
cargo test -p soma-gateway --no-default-features net
```

## Phase 5: Process and Stdio Transport Cleanup (`rmcp-template-0lnb.5`)

Port process hygiene before full upstream pool behavior.

Files:

- `crates/soma-gateway/src/process.rs`
- `crates/soma-gateway/src/process_tests.rs`
- `crates/soma-gateway/src/process/guard.rs`
- `crates/soma-gateway/src/process/guard_tests.rs`
- `crates/soma-gateway/src/process/stdio.rs`
- `crates/soma-gateway/src/process/stdio_tests.rs`
- `crates/soma-gateway/src/process/stderr.rs`
- `crates/soma-gateway/src/process/stderr_tests.rs`
- `crates/soma-gateway/src/process/windows.rs`
- `crates/soma-gateway/src/process/windows_tests.rs`
- `crates/soma-gateway/src/upstream/pool/connect_stdio.rs`
- `crates/soma-gateway/src/upstream/pool/connect_stdio_tests.rs`

Requirements:

- `Tokio Child` drop does not kill the process; process cleanup is mandatory.
- Unix process group cleanup.
- Windows Job Object cleanup without `labby-winjob`.
- Stderr draining cannot deadlock on saturation.
- Cache repair filesystem traversal/deletion/writes run under `spawn_blocking`.
- `gateway.test`, add/update/import, and pending-import approval all run identical spawn/env validation before spawning.

Run:

```bash
cargo test -p soma-gateway --no-default-features process
cargo test -p soma-gateway --no-default-features stdio
```

If a `windows-process` feature is introduced:

```bash
cargo test -p soma-gateway --features windows-process
```

## Phase 6: Upstream Pool Core (`rmcp-template-0lnb.6`)

Mandatory internal order:

1. Pool skeleton plus in-process/mock call-through.
2. HTTP/SSE caps.
3. Stdio connector integration from Phase 5.
4. WebSocket supported-or-unsupported decision.
5. Tools/resources/prompts groups.

Do not port resources/prompts before tool discovery plus one routed call are green.

Files include:

- `crates/soma-gateway/src/upstream.rs`
- `crates/soma-gateway/src/upstream_tests.rs`
- `crates/soma-gateway/src/upstream/http_client.rs`
- `crates/soma-gateway/src/upstream/http_client_tests.rs`
- `crates/soma-gateway/src/upstream/pool.rs`
- `crates/soma-gateway/src/upstream/pool_tests.rs`
- `crates/soma-gateway/src/upstream/pool/discovery.rs`
- `crates/soma-gateway/src/upstream/pool/discovery_tests.rs`
- `crates/soma-gateway/src/upstream/pool/tools.rs`
- `crates/soma-gateway/src/upstream/pool/tools_tests.rs`
- `crates/soma-gateway/src/upstream/pool/resources.rs`
- `crates/soma-gateway/src/upstream/pool/resources_tests.rs`
- `crates/soma-gateway/src/upstream/pool/prompts.rs`
- `crates/soma-gateway/src/upstream/pool/prompts_tests.rs`
- `crates/soma-gateway/src/upstream/pool/health.rs`
- `crates/soma-gateway/src/upstream/pool/health_tests.rs`

Do not copy Lab's process-wide `OnceLock` config knobs. Active cap/concurrency values live on manager/pool config and change on reload.

Caps must cover:

- `tools/list`
- `tools/call`
- `resources/list`
- `resources/read`
- `prompts/list`
- `prompts/get`
- relay `call_tool`
- HTTP JSON
- HTTP SSE per-event
- WebSocket frame/message, if supported
- stdio post-decode

Subject-scoped paths use the same discovery concurrency cap as bulk discovery:

- subject-scoped tools
- prompts
- prompt owner lookup
- OAuth status refresh
- Code Mode catalog refresh
- palette catalog/schema

Run after each group:

```bash
cargo test -p soma-gateway --no-default-features upstream
cargo test -p soma-gateway --no-default-features discovery
cargo test -p soma-gateway --no-default-features caps
find crates/soma-gateway -type f -name '*.rs' -print0 | xargs -0 -n1 wc -l | awk '$1 > 500 { bad=1; print } END { exit bad }'
```

## Phase 7: Relay Sessions and Subject Cache (`rmcp-template-0lnb.7`)

Relay cache key is exactly `(upstream, session_id, subject)`.

Relay session IDs are gateway-minted per downstream MCP session, never accepted from user params or forgeable `mcp-session-id` headers.

Relay applies to `call_tool` only unless deliberately widened.

Files:

- `crates/soma-gateway/src/upstream/relay.rs`
- `crates/soma-gateway/src/upstream/relay_tests.rs`
- `crates/soma-gateway/src/upstream/relay/cache.rs`
- `crates/soma-gateway/src/upstream/relay/cache_tests.rs`
- `crates/soma-gateway/src/upstream/relay/session.rs`
- `crates/soma-gateway/src/upstream/relay/session_tests.rs`
- `crates/soma-gateway/src/upstream/relay/lifecycle.rs`
- `crates/soma-gateway/src/upstream/relay/lifecycle_tests.rs`

Tests:

- Same upstream/session with different subjects creates distinct connections.
- Same subject with different sessions creates distinct connections.
- Same key burst single-flights.
- Leader cancellation, failed connect, waiter cancellation.
- TTL eviction, LRU over-cap eviction, dead transport eviction.
- Lock map returns to zero or live cached entries after failure/cancel/sweep.
- Evicted transports graceful-shutdown off-lock.
- Forged session IDs cannot reuse another relay connection.
- Mirrored capabilities checked for elicitation, sampling, and roots.

Run:

```bash
cargo test -p soma-gateway --no-default-features relay
```

## Phase 8: Gateway Manager and Usage Seam (`rmcp-template-0lnb.9`)

`GatewayManager` owns lifecycle state. `UpstreamPool` owns transport state.

Files:

- `crates/soma-gateway/src/gateway/manager.rs`
- `crates/soma-gateway/src/gateway/manager_tests.rs`
- `crates/soma-gateway/src/gateway/manager/core.rs`
- `crates/soma-gateway/src/gateway/manager/core_tests.rs`
- `crates/soma-gateway/src/gateway/manager/pool_lifecycle.rs`
- `crates/soma-gateway/src/gateway/manager/pool_lifecycle_tests.rs`
- `crates/soma-gateway/src/gateway/runtime.rs`
- `crates/soma-gateway/src/gateway/runtime_tests.rs`
- `crates/soma-gateway/src/gateway/projection.rs`
- `crates/soma-gateway/src/gateway/projection_tests.rs`
- `crates/soma-gateway/src/registry.rs`
- `crates/soma-gateway/src/registry_tests.rs`
- `crates/soma-gateway/src/usage.rs`
- `crates/soma-gateway/src/usage_tests.rs`

Start usage recording with:

```rust
pub trait UsageSink: Send + Sync {
    fn record(&self, event: UsageEvent);
}

pub struct NoopUsageSink;
```

Port SQLite only if user-facing usage actions require it, keep it feature-gated/local, and avoid pulling SQLite into the no-default manager path.

Gateway-local service metadata:

- `GatewayServiceMeta`
- `GatewayServiceAction`
- `GatewayEnvVar`
- optional in-process peer traits

Product/provider adapters map into these DTOs; `soma-gateway` does not import `soma-service`, `soma-contracts`, or Lab `PluginMeta`.

Full rebuild behavior:

- Preferred: build/warm fresh pool, atomically swap in, drain old off routing path.
- If overlap is unsafe, concurrent calls fail fast with structured `gateway_reloading`.

Run:

```bash
cargo test -p soma-gateway --no-default-features manager
cargo test -p soma-gateway --no-default-features runtime
cargo test -p soma-gateway --no-default-features usage
```

If SQLite usage lands, add `EXPLAIN QUERY PLAN` tests at 100k seeded rows for metrics and call listings.

## Phase 9: OAuth Wiring (`rmcp-template-0lnb.8`)

Feature: `oauth`.

`soma-gateway` adapts gateway-local upstream config into:

```rust
soma_auth::upstream::config::UpstreamConfig
```

`soma-auth` must not import gateway config.

Add an identity matrix with columns:

- surface
- authenticated caller/audit owner
- upstream credential subject
- relay cache subject
- protected public-route caller subject
- source of truth
- whether caller-supplied `subject` is accepted

Rules:

- Caller-supplied `subject` accepted only on admin OAuth operations.
- Code Mode admin/shared callers use shared gateway subject.
- Protected routes never forward public bearer tokens upstream.
- Protected routes use shared gateway credentials when upstream OAuth is needed.
- OAuth tests are overlays and must not make `codemode`, `openapi`, or `palette` require `oauth`.

Run:

```bash
cargo test -p soma-gateway --features oauth
cargo tree -p soma-gateway --features oauth -e features
cargo tree -p soma-gateway --features oauth | rg 'labby-' && exit 1 || true
```

## Phase 10: Protected Routes and Virtual Servers (`rmcp-template-0lnb.10`)

This phase ports route config/index/scope and virtual-server projection only. Product HTTP interception/proxying/scoped MCP router construction stays in Phase 13.

Files:

- `crates/soma-gateway/src/gateway/protected_routes.rs`
- `crates/soma-gateway/src/gateway/protected_routes_tests.rs`
- `crates/soma-gateway/src/gateway/virtual_servers.rs`
- `crates/soma-gateway/src/gateway/virtual_servers_tests.rs`
- `crates/soma-gateway/src/gateway/manager/protected_routes.rs`
- `crates/soma-gateway/src/gateway/manager/protected_routes_tests.rs`
- `crates/soma-gateway/src/gateway/manager/virtual_servers.rs`
- `crates/soma-gateway/src/gateway/manager/virtual_servers_tests.rs`

Tests:

- Host normalization with port, trailing dot, comma-separated values, and rejected spoofing.
- Path prefix boundaries: `/mcp2`, encoded slash/dot segments, exact metadata paths.
- Backend URL policy at config-write and dispatch time.
- No backend URL in metadata/challenges/errors.
- Health-aware connected/projection behavior.
- Public request params cannot define trusted protected-route scope.

Run:

```bash
cargo test -p soma-gateway --features protected-routes
```

## Phase 11: Code Mode, OpenAPI, Palette, Journal Adapters (`rmcp-template-0lnb.11`)

Blocked on `rmcp-template-ehml.8` unless exact closed child outputs are proven sufficient.

This phase ports gateway-specific adapters only. If engine behavior is missing from `soma-codemode` or `soma-openapi`, finish `rmcp-template-ehml` first.

Files:

- `crates/soma-gateway/src/gateway/code_mode.rs`
- `crates/soma-gateway/src/gateway/code_mode_tests.rs`
- `crates/soma-gateway/src/gateway/code_mode/host.rs`
- `crates/soma-gateway/src/gateway/code_mode/host_tests.rs`
- `crates/soma-gateway/src/gateway/code_mode/catalog.rs`
- `crates/soma-gateway/src/gateway/code_mode/catalog_tests.rs`
- `crates/soma-gateway/src/codemode_journal.rs`
- `crates/soma-gateway/src/codemode_journal_tests.rs`
- `crates/soma-gateway/src/gateway/palette.rs`
- `crates/soma-gateway/src/gateway/palette_tests.rs`
- `crates/soma-gateway/src/gateway/openapi.rs`
- `crates/soma-gateway/src/gateway/openapi_tests.rs`

Subphase order:

1. `codemode` without `oauth`.
2. `openapi` adapter.
3. `palette` gateway behavior.
4. OAuth-subject overlay tests.

Exact validation commands:

```bash
cargo test -p soma-gateway --features codemode
cargo test -p soma-gateway --features openapi
cargo test -p soma-gateway --features codemode,openapi
cargo test -p soma-gateway --features palette
cargo test -p soma-gateway --features codemode,oauth
cargo tree -p soma-gateway --features codemode -e features
cargo tree -p soma-gateway --features codemode,openapi -e features
```

Tests:

- Namespace IDs are `<namespace>::<tool>`.
- Params must be objects.
- Error kinds preserved.
- Cold catalog rendering avoids upstream connects.
- UI link capture preserved.
- Code Mode catalog schema cap 512 KiB.
- Palette schema projection cap 64 KiB.
- Product palette HTTP headers/cache stay in Phase 13.
- Repeated `palette_catalog` within freshness window issues zero upstream reprobes/list calls.
- `palette_schema` resolves only requested schema.
- 1000-tool / 64 KiB-schema no-deep-clone test for names-only/help/catalog/cache hits.

## Phase 12: Action Catalog, Params, Dispatch, Views (`rmcp-template-0lnb.12`)

There is one gateway action catalog and one product auth policy.

`soma-gateway` does not parse bearer tokens, JWTs, or scopes. Product/MCP/HTTP shims pass an already-authenticated `GatewayPrincipal` / `GatewayAccess`.

Files:

- `crates/soma-gateway/src/gateway/catalog.rs`
- `crates/soma-gateway/src/gateway/catalog_tests.rs`
- `crates/soma-gateway/src/gateway/catalog/*.rs`
- `crates/soma-gateway/src/gateway/dispatch.rs`
- `crates/soma-gateway/src/gateway/dispatch_tests.rs`
- `crates/soma-gateway/src/gateway/dispatch/*.rs`
- `crates/soma-gateway/src/gateway/params.rs`
- `crates/soma-gateway/src/gateway/params_tests.rs`
- `crates/soma-gateway/src/gateway/view_models.rs`
- `crates/soma-gateway/src/gateway/view_models_tests.rs`
- `crates/soma-gateway/src/dispatch_helpers.rs`
- `crates/soma-gateway/src/dispatch_helpers_tests.rs`

Rules:

- All non-discovery gateway actions have executable admin requirements.
- Unknown gateway actions fail closed as admin-required/denied.
- Destructive action metadata is executable test data.
- `gateway.test`, add/update/import, and pending-import approval run spawn/env validation before process spawn.
- `proxy_resources` and `proxy_prompts` omission defaults true.
- Explicit false remains valid.
- `expose_* = null` means no filter.
- Params must be objects.
- Stable gateway error kinds and structured tool errors.

Run:

```bash
cargo test -p soma-gateway --all-features dispatch
cargo test -p soma-gateway --all-features catalog
cargo test -p soma-gateway --all-features params
cargo test -p soma-gateway --all-features view
```

## Phase 13: Product Integration (`rmcp-template-0lnb.13`)

Product crates may depend on `soma-gateway`; `soma-gateway` must not depend back on product crates.

Files:

- `crates/soma-contracts/src/**`
- `crates/soma-runtime/src/server.rs`
- `crates/soma/src/runtime.rs`
- `crates/soma/src/routes.rs`
- `crates/soma-api/src/api.rs`
- `crates/soma-mcp/src/**` only for gateway MCP exposure.
- `crates/soma-cli/src/**` only for thin gateway/admin commands if required.
- `crates/soma/tests/**`

Admin scope:

```rust
pub const ADMIN_SCOPE: &str = "soma:admin";
```

Define `ADMIN_SCOPE` in `soma-contracts`, advertise it in OAuth supported scopes, and make static-bearer admin behavior explicit:

- Either add distinct admin token/scope config.
- Or document that static bearer cannot administer gateway routes.

Tests:

- `soma:read` denied for gateway admin.
- `soma:admin` allowed.
- `SOMA_MCP_TOKEN` read-only cannot call admin actions.
- OAuth token with `soma:admin` allowed.
- Loopback dev and trusted-gateway bypass tests do not count as mounted admin enforcement proof.
- Non-loopback no-auth rejected unless deliberately trusted.
- Public health unaffected.
- Gateway body-limit behavior deliberate for config/import/schema endpoints.

Integration seam:

- Exactly one `Arc<GatewayManager>` or `GatewayProductState` handle behind a gateway feature.
- Product crates must not independently instantiate `UpstreamPool`, persist `GatewayConfig`, or maintain gateway catalogs except in tests.
- Protected route interception runs after route auth and before SPA/fallback.
- Strip inbound `Authorization` before upstream auth is chosen.
- Forward only explicit MCP/content header allowlist.

Run:

```bash
cargo test -p soma --test architecture_boundaries
cargo test -p soma --test tool_dispatch
cargo test --workspace
```

## Phase 14: Final Gates and Closeout (`rmcp-template-0lnb.14`)

Final gate re-verifies child-local proofs. It must not be the first place feature, LOC, sibling-test, or dependency violations are discovered.

Run:

```bash
cargo fmt --all --check
cargo test -p soma-gateway --no-default-features
cargo test -p soma-gateway --features oauth
cargo test -p soma-gateway --features codemode
cargo test -p soma-gateway --features openapi
cargo test -p soma-gateway --features codemode,openapi
cargo test -p soma-gateway --features palette
cargo test -p soma-gateway --all-features
cargo clippy -p soma-gateway --all-targets --all-features -- -D warnings
cargo test --workspace

cargo xtask check-test-siblings
cargo xtask patterns
cargo xtask check-file-size
cargo xtask check-version-sync
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

Fresh target feature matrix:

```bash
fresh_target="$(mktemp -d /tmp/soma-gateway-target.XXXXXX)"
RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR="$fresh_target" cargo test -p soma-gateway --no-default-features --locked
RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR="$fresh_target" cargo test -p soma-gateway --features oauth --locked
RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR="$fresh_target" cargo test -p soma-gateway --features codemode --locked
RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR="$fresh_target" cargo test -p soma-gateway --features openapi --locked
RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR="$fresh_target" cargo test -p soma-gateway --features codemode,openapi --locked
RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR="$fresh_target" cargo test -p soma-gateway --all-features --locked
```

Dependency drift checks:

```bash
cargo tree -p soma-gateway --no-default-features -e features
cargo tree -p soma-gateway --features oauth -e features
cargo tree -p soma-gateway --features codemode -e features
cargo tree -p soma-gateway --features openapi -e features
cargo tree -p soma-gateway --features codemode,openapi -e features
cargo tree -p soma-gateway --all-features -e features
cargo tree -p soma-gateway --all-features -i rmcp@2.2.0
cargo tree -p soma-gateway --all-features -i reqwest@0.13.4
cargo tree -p soma-gateway --all-features | rg 'labby-|reqwest v0\\.12|rusqlite v0\\.39|libsqlite3-sys v0\\.37' && exit 1 || true
```

LOC gate including tests:

```bash
git diff --name-only origin/main...HEAD -- '*.rs' \
  | xargs -r -n1 wc -l \
  | awk '$1 > 500 { bad=1; print } END { exit bad }'

find crates/soma-gateway -type f -name '*.rs' -print0 \
  | xargs -0 -n1 wc -l \
  | awk '$1 > 500 { bad=1; print } END { exit bad }'
```

Post-review correction, folded back into execution:

- The earlier "foundation only" acceptance state was rejected. The branch must implement and verify the full self-contained gateway-owned port instead of preserving fake or partial parity language.
- Gateway-owned live transport parity means real async rmcp routing for HTTP/SSE, stdio, and websocket upstreams; live discovery and calls; tools/resources/prompts proxying; protected route scoping; public bearer isolation; subject-scoped OAuth routing; upstream OAuth lifecycle actions; strict config validation; filesystem persistence; product startup load path; admin add/update/remove/reload/import/test mutation; safe redaction; and route inventory/OpenAPI exposure for `/v1/gateway/{action}`.
- Code Mode and OpenAPI engine behavior remains behind the feature-gated adapter seams because those are separate crates being ported. The gateway crate must not depend on Labby or Soma product crates to fake those surfaces.
- Final smoke must prove the stronger live gateway contract: disposable upstream `soma-gateway-smoke`, discovered/exposed counts, real routed echo returning `smoke-0lnb`, protected-route no-leak proof, public bearer isolation, OAuth subject miss, relay cross-session isolation, stdio child/grandchild cleanup, redaction scan over captured smoke logs/results, read-only bearer discovery, admin denial for read-only tokens, loopback/admin add-list-remove behavior, config view no protected backend leak, and no placeholder success for unknown or unimplemented actions.

Broad 20+ mixed-upstream concurrency/stress remains deterministic test coverage for the live transports; it is not a substitute for the small live smoke.

Closeout:

```bash
bd swarm validate rmcp-template-0lnb
bd close rmcp-template-0lnb.1 rmcp-template-0lnb.2 rmcp-template-0lnb.3 rmcp-template-0lnb.4 rmcp-template-0lnb.5 rmcp-template-0lnb.6 rmcp-template-0lnb.7 rmcp-template-0lnb.8 rmcp-template-0lnb.9 rmcp-template-0lnb.10 rmcp-template-0lnb.11 rmcp-template-0lnb.12 rmcp-template-0lnb.13 rmcp-template-0lnb.14 --reason "Implemented and verified the self-contained gateway-owned Labby port"
bd close rmcp-template-0lnb --reason "Self-contained gateway-owned Labby port is implemented and verified"
git pull --rebase
bd dolt push
git push
git status --short --branch
```

Do not run the closeout commands above while review blockers remain. Close reasons must stay explicit that this is the gateway-owned Labby port and that Code Mode/OpenAPI engines are separate feature-gated crates.

## Review Workflow After Implementation

Per `vibin:work-it`, open a PR as soon as the plan is locally green enough to review, then run:

- `lavra-review`
- three `code_simplifier` passes, one at a time
- all available PR review toolkit agents
- resolve all actionable review comments
- re-run final gates
- save a session note
- push final branch

Do not close the epic until implementation, review, validation, Beads push, Git push, and final clean/up-to-date status are all complete.
