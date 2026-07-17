# Soma Architecture Refactor Plan, Revision 3

**Repository:** `jmagar/soma`
**Date:** 2026-07-15
**Status:** All plan slices (PR 0 through PR 19) completed; see the execution
ledger below for merge evidence and two carried-forward follow-ups tracked in
beads (`rmcp-template-f1ll` freeze-audit gaps, `rmcp-template-ns29`
brand-neutral shared-crate rename)
**Supersedes:** `soma-architecture-refactor-plan-v2.md`

## Execution status

Last updated: 2026-07-17.

This ledger is the source of truth for plan execution. "Completed" means the
slice's acceptance boundary is implemented and reviewed; merge evidence names
the GitHub PRs that landed it. PR 1 was intentionally satisfied as a distributed
safety-net effort rather than a standalone pull request. PRs 10-18 landed as a
stacked branch chain (each based on the previous slice's branch); their PR
numbers below are open GitHub PRs against that chain, not yet merged to
`main` — the work itself is implemented and reviewed per this ledger's
"Completed" definition, and PR 19 (this slice) builds directly on top of
`refactor/pr18-slim-app`.

| Plan slice | Status | Merge evidence or remaining outcome |
|---|---|---|
| PR 0 - merge preparation and shared-crate corrections | Completed, with carryover | Traces #134 and shared MCP/gateway foundations #141, plus the reviewed gateway landing. Carryover: section 9.1's `[workspace.dependencies]` centralization was never built; reassigned to PR 10 |
| PR 1 - freeze post-merge behavior | Completed (distributed), unaudited | Characterization, contract, profile, schema, parity, and compatibility gates were expanded and run throughout PRs 0 and 2-9. The freeze was built alongside the moves rather than ahead of them; audit coverage before PR 12 |
| PR 2 - physical workspace taxonomy | Completed | #139 |
| PR 3 - architecture enforcement | Completed | #140 |
| PR 4 - `soma-domain` and `soma-application` | Completed | #143 |
| PR 5 - CLI through `SomaApplication` | Completed | #144 |
| PR 6 - REST through `SomaApplication` | Completed | #145 |
| PR 7 - MCP through `SomaApplication` | Completed | #146 and stdio/profile follow-up #147 |
| PR 8 - runtime application facade | Completed | #148 |
| PR 9 - extract canonical `soma-provider-core` | Completed | #149; shared-only provider contracts/registry with Soma as a product-policy wrapper |
| PR 10 - provider adapters and engine reconciliation | Completed | #152 (also opened against `main` as #153) |
| PR 11 - `soma-integrations` | Completed | #154 |
| PR 12 - split `soma-service` | Completed | #155 (business workflows, provider registry, capabilities, and drop-in providers moved into `soma-application`; PR 19 finished the crate deletion — see its own row) |
| PR 13 - split `soma-contracts` | Completed | #156 (actions/errors/scopes/token_limit/provider_validation to `soma-domain`, config/env_registry to `soma-config`, provider metadata to `soma-provider-core`; PR 19 finished the crate deletion) |
| PR 14 - finish MCP role-crate cleanup | Completed | #157 |
| PR 15 - extract `soma-http-server` | Completed | #158 |
| PR 16 - extract `soma-cli-core` | Completed | #159 |
| PR 17 - Palette and Tauri split | Completed | #160 |
| PR 18 - slim `apps/soma` | Completed | #161 |
| PR 19 - delete legacy facades and update artifacts | Completed | bead `rmcp-template-cfi0`. Deleted `crates/soma/service` (moved its remaining business logic — `SomaService`, `ProviderRegistry`, `CapabilityBroker`, drop-in providers — into `soma-application`, since PR 12's own slice left that migration unfinished) and `crates/soma/contracts` (already a pure re-export facade after PR 13; deleted with no consumer migration needed). Removed both `TEMPORARY_EXCEPTIONS` entries from `xtask/src/architecture.rs`; `cargo xtask check-architecture` now runs with zero exceptions. Updated every ecosystem artifact that named the deleted crates (README, docs/ARCHITECTURE.md, CLAUDE.md module map, xtask path constants, scaffold output, generated-docs generators, plugin/doc cross-references) — see PR body for the full list |

Current stopping point: PR 19 is the last slice in this plan. The refactor's
"Definition of done" (section 13) is satisfied — `soma-service` and
`soma-contracts` are gone, the architecture checker has zero temporary
exceptions, and all gates (fmt, clippy, nextest, check-architecture,
check-version-sync, the three feature-profile builds, and the optional xtask
doc/schema/plugin-layout gates) are green. Remaining follow-on work is tracked
in beads, not this plan: `rmcp-template-ns29` (brand-neutral shared-crate
rename — see the note below) and `rmcp-template-f1ll` (freeze-audit gap
closure).

### Open risks carried into PR 10 and later

1. **The behavior freeze was built after the moves it was meant to protect.**
   Section 14 ordered PR 1 second, ahead of the physical taxonomy, so that later
   refactors could prove behavior did not change. It was instead satisfied
   incrementally across PRs 0 and 2-9. That was survivable for slices that were
   mostly mechanical moves. PRs 12, 13, and 19 delete `soma-service` and
   `soma-contracts` and drop their compatibility re-exports — exactly what the
   freeze exists to guard. Audit the PR 1 snapshot list for real coverage before
   PR 12 starts deleting, rather than assuming "distributed" means complete.
   **Status:** tracked as its own follow-up in bead `rmcp-template-f1ll`
   (freeze-audit gap closure) rather than blocking PR 19; PR 19 verified
   behavior preservation for its own slice via the full gate suite (fmt,
   clippy, nextest — 1582 tests — check-architecture, the three feature-profile
   builds, and the optional doc/schema/plugin-layout gates) plus a manual
   review of every moved module's call sites, but did not re-run a dedicated
   PR 1 characterization/parity audit across the whole PR 12/13/19 deletion
   span. See the bead's comments for how that gap is being closed.

2. **Shared crate publish names are still Soma-branded — rename decided, names
   pending.** Twelve of sixteen shared crates carry `soma-*` package names.
   PR 19 rewrites the README, `docs/ARCHITECTURE.md`, scaffold output, the
   cargo-generate template, generated provider docs, plugin metadata,
   Dockerfiles, and CI workflows; a rename after PR 19 would redo all of it.
   **Decided 2026-07-16:** the shared crates get brand-neutral names in a
   dedicated rename slice that lands after PR 16 (once all shared crates
   exist) and before PR 19 (so the artifact sweep writes final names once).
   Choosing the concrete names is a separate exercise tracked in beads; until
   that slice lands, `soma-*` names remain valid in-repo and must not be
   published to crates.io.
   **Status as of PR 19:** the ordering precondition is now satisfied — PR 16
   (`soma-cli-core`, the last shared crate created by this plan) and PR 19
   (this artifact sweep) have both landed, so every shared crate this rename
   touches now exists and every ecosystem artifact the rename would otherwise
   need to rewrite twice has already been written once with final (`soma-*`)
   names. The rename slice is unblocked and tracked in bead
   `rmcp-template-ns29`; it did not land inside PR 19 itself (out of this
   slice's scope — PR 19 deletes legacy facades and updates artifacts for the
   current names, it does not choose or apply new package names).

This revision adopts the physical workspace taxonomy selected for Soma:

```text
apps/
└── soma/                 executable composition

crates/
├── shared/               reusable across unrelated projects
└── soma/                 Soma-specific product libraries
```

The directory is an architectural signal, not decoration:

- `apps/soma` contains the process entry point and composition root.
- `crates/shared/*` and nested shared groups such as `crates/shared/mcp/*` may be reused by another product without importing Soma product code.
- `crates/soma/*` contains product behavior, policies, routes, tools, commands, and adapters specific to Soma.

The core operational rule remains:

```text
CLI command ─┐
REST route ──┼──▶ soma-application use case
MCP tool ────┘
```

A surface translates input and output. It does not independently implement the business operation.

## Reusable crate goal

The shared layer is not merely internal cleanup. This repository should become the source of truth for Jacob's extracted Rust project building blocks and the proof of concept for the facade/orchestrator pattern.

The goal is to extract recurring Rust project patterns into reusable crates that can be pulled into new projects quickly, mixed and matched independently, and eventually published to crates.io.

For a new project, the target experience is:

```text
new product
    depends on shared auth, observability, API, web, CLI, MCP, provider, gateway,
    OpenAPI, Code Mode, and trace crates

product code
    supplies configuration, branding/defaults, domain types, application use cases,
    product-specific routes/tools/commands, and integrations between the shared crates
```

Soma becomes the reference implementation and packaging/runtime product built on these crates. New products should mostly compose the shared crates behind a thin facade/orchestrator and then add domain-specific behavior.

Every shared crate should be evaluated for:

- scaffold speed: can a new product use it with explicit config and minimal glue?
- reuse: can an unrelated Rust project depend on it without importing Soma product code?
- publishability: are package names, docs, defaults, features, and dependency graphs suitable for crates.io?
- mix-and-match simplicity: can a consumer opt into only the crate and feature set they need without inheriting the whole Soma stack?

Prefer small explicit configuration types, narrow feature flags, examples that start from a blank project, and dependency graphs that stay understandable in `cargo tree`. Avoid hidden product defaults, broad feature aggregators, and "one crate imports everything" convenience shortcuts in shared crates.

Dependency independence is a product goal, not just an aesthetic preference:

- Leaf shared crates should have no internal Soma workspace dependencies unless there is a clear shared abstraction they are intentionally building on.
- Higher-level shared crates may depend on lower-level shared crates, but only across explicit layers and only when the dependency keeps the public API simpler or preserves a shared invariant.
- Optional features should be preferred for integrations such as Axum, rmcp, OpenAPI, storage, tracing exporters, OAuth providers, and gateway/auth bridges.
- No shared crate should require a product crate, product defaults, product env prefixes, product binary names, or product policy to compile or to use its default feature set.
- A little duplication at crate edges is acceptable when it prevents a tiny helper from forcing a large transitive dependency tree on consumers.

The practical target is not "every crate depends on nothing." It is "every crate can be understood, packaged, documented, and adopted without dragging in unrelated product surface."

---

## 1. Executive decision

Adopt this as the canonical target:

```text
soma/
├── Cargo.toml
├── Cargo.lock
├── Justfile
├── xtask/
│
├── apps/
│   ├── soma/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── bootstrap.rs
│   │   │   ├── invocation.rs
│   │   │   ├── local.rs
│   │   │   ├── http.rs
│   │   │   ├── stdio.rs
│   │   │   ├── shutdown.rs
│   │   │   └── bin/
│   │   │       ├── soma.rs
│   │   │       ├── soma-api.rs          # optional convenience binary
│   │   │       └── soma-mcp.rs          # optional convenience binary
│   │   └── tests/
│   │       ├── bootstrap.rs
│   │       ├── feature_profiles.rs
│   │       └── process_smoke.rs
│   │
│   ├── web/                              # existing editable frontend source
│   └── palette/                          # desktop Palette frontend + Tauri app package
│       ├── src/
│       └── src-tauri/                    # app-local Tauri composition package
│
└── crates/
    ├── shared/
    │   ├── traces/
    │   ├── auth/
    │   ├── observability/
    │   ├── openapi/
    │   ├── codemode/
    │   ├── http-api/
    │   ├── mcp/
    │   │   ├── client/
    │   │   ├── server/
    │   │   ├── proxy/
    │   │   └── gateway/
    │   ├── provider-core/
    │   ├── provider-adapters/
    │   ├── http-server/
    │   ├── cli-core/
    │   ├── tauri-shell/
    │   └── codex-app-server-client/
    │
    └── soma/
        ├── domain/
        ├── application/
        ├── config/
        ├── client/
        ├── integrations/
        ├── runtime/
        ├── api/
        ├── mcp/
        ├── cli/
        ├── palette/
        ├── test-support/
        └── web/
```

Do not add `crates/soma/gateway` by default. Use `crates/soma/integrations` for the initial Soma-to-gateway auth/config/policy adapters. Create `crates/soma/gateway` only if those adapters become a durable product gateway facade rather than thin integration glue.

The optional `soma-api` and `soma-mcp` binaries are not required to achieve separate launch modes. The canonical `soma` binary can continue to provide:

```bash
soma serve
soma mcp
soma <command>
```

Add additional binaries only when they improve packaging, operational policy, or deployment. Do not add them merely because the package can emit them.

### Classification rule

A crate belongs in `crates/shared` only when all of the following are true:

1. It has no dependency on `crates/soma/*` or `apps/*`.
2. Its public constructors work without Soma configuration or runtime state.
3. Its public types do not encode Soma product policy.
4. An unrelated Rust project could use it naturally.
5. Its all-features dependency graph still satisfies the previous rules.
6. Its default feature set is minimal enough that consumers can mix and match crates without pulling in unrelated surfaces.

A crate belongs in `crates/soma` when it defines or implements Soma-specific behavior, even when several Soma binaries or surfaces reuse it.

---

## 2. Directory names, package names, and Rust import names

The physical path determines the architectural layer. The package name determines Cargo identity. The Rust crate name is the package name with hyphens converted to underscores.

| Path | Cargo package | Rust import | Classification |
|---|---|---|---|
| `apps/soma` | `soma` | `soma` | executable composition |
| `crates/shared/traces` | `rmcp-traces` | `rmcp_traces` | shared |
| `crates/shared/auth` | `soma-auth` | `soma_auth` | shared |
| `crates/shared/observability` | `soma-observability` | `soma_observability` | shared |
| `crates/shared/openapi` | `soma-openapi` | `soma_openapi` | shared |
| `crates/shared/codemode` | `soma-codemode` | `soma_codemode` | shared |
| `crates/shared/http-api` | `soma-http-api` | `soma_http_api` | shared |
| `crates/shared/mcp/client` | `soma-mcp-client` | `soma_mcp_client` | shared |
| `crates/shared/mcp/server` | `soma-mcp-server` | `soma_mcp_server` | shared |
| `crates/shared/mcp/proxy` | `soma-mcp-proxy` | `soma_mcp_proxy` | shared |
| `crates/shared/mcp/gateway` | `soma-gateway` | `soma_gateway` | shared |
| `crates/shared/provider-core` | `soma-provider-core` | `soma_provider_core` | shared |
| `crates/shared/provider-adapters` | `soma-provider-adapters` | `soma_provider_adapters` | shared |
| `crates/shared/http-server` | `soma-http-server` | `soma_http_server` | shared |
| `crates/shared/cli-core` | `soma-cli-core` | `soma_cli_core` | shared |
| `crates/shared/tauri-shell` | `soma-tauri-shell` | `soma_tauri_shell` | shared |
| `crates/shared/codex-app-server-client` | `codex-app-server-client` | `codex_app_server_client` | shared |
| `apps/palette/src-tauri` | `soma-palette-tauri` | `soma_palette_tauri` | executable composition |
| `crates/soma/domain` | `soma-domain` | `soma_domain` | product |
| `crates/soma/application` | `soma-application` | `soma_application` | product |
| `crates/soma/config` | `soma-config` | `soma_config` | product |
| `crates/soma/client` | `soma-client` | `soma_client` | product |
| `crates/soma/integrations` | `soma-integrations` | `soma_integrations` | product |
| `crates/soma/runtime` | `soma-runtime` | `soma_runtime` | product |
| `crates/soma/api` | `soma-api` | `soma_api` | product |
| `crates/soma/mcp` | `soma-mcp` | `soma_mcp` | product |
| `crates/soma/cli` | `soma-cli` | `soma_cli` | product |
| `crates/soma/palette` | `soma-palette` | `soma_palette` | product |
| `crates/soma/test-support` | `soma-test-support` | `soma_test_support` | product |
| `crates/soma/web` | `soma-web` | `soma_web` | product |

The nested path is the architectural signal. Existing incoming package names may remain unchanged during the migration to reduce Cargo churn. Brand-neutral shared package names are **decided** (2026-07-16): the rename happens in a dedicated slice after PR 16 and before PR 19, so PR 19's ecosystem-artifact sweep writes the final names exactly once. Concrete name selection is tracked as its own bead; nothing ships to crates.io under a `soma-*` name.

Avoid `-kit` for shared crate names unless the crate is truly a loose grab bag. Prefer `*-core` for foundational contracts that other crates build around, `*-adapters` for concrete implementations, `*-client`/`*-server`/`*-proxy` for protocol roles, and concrete purpose names such as `http-api` or `http-server` when the boundary is obvious.

---

## 3. Full target workspace

## 3.1 `apps/soma`: composition root and binary package

Suggested source layout:

```text
apps/soma/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── bootstrap.rs
│   ├── invocation.rs
│   ├── local.rs
│   ├── http.rs
│   ├── stdio.rs
│   ├── shutdown.rs
│   └── bin/
│       ├── soma.rs
│       ├── soma-api.rs       # optional
│       └── soma-mcp.rs       # optional
└── tests/
    ├── bootstrap.rs
    ├── feature_profiles.rs
    └── process_smoke.rs
```

### Owns

- loading top-level product configuration
- initializing tracing and metrics
- constructing concrete clients, engines, repositories, and adapters
- constructing `SomaApplication`
- constructing `SomaRuntime`
- selecting CLI, stdio MCP, or HTTP server mode
- composing REST, HTTP MCP, auth, metrics, and web routers
- binding listeners
- operating-system signals
- task startup and graceful process shutdown
- process exit codes
- top-level Cargo feature aggregation

### Does not own

- provider business logic
- authorization rules
- destructive-action policy
- action execution workflows
- gateway business workflows
- HTTP request/response DTO definitions
- MCP tool schemas
- CLI command implementation
- generic web, CLI, or MCP framework code

### Suggested responsibilities by file

```text
bootstrap.rs
    Load SomaConfig.
    Build auth implementation.
    Build provider registry and adapters.
    Build gateway, Code Mode, and OpenAPI adapters.
    Build SomaApplication.
    Build SomaRuntime.

invocation.rs
    Convert the top-level CLI parser result into an execution mode.
    Keep mode selection separate from command business logic.

local.rs
    Run one-shot CLI commands against Arc<SomaApplication>.

http.rs
    Merge soma_api::router(...), soma_mcp::http_router(...),
    soma_palette::router(...), auth routes, observability routes,
    and soma_web fallback.
    Call soma_http_server::serve(...).

stdio.rs
    Construct the Soma MCP adapter and call soma_mcp_server stdio lifecycle.

shutdown.rs
    Build and propagate CancellationToken or equivalent process shutdown.

bin/soma.rs
    Minimal process entry point.
```

Example binary entry point:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    soma::run(std::env::args_os()).await
}
```

The composition root may depend on every required product crate and shared engine. No lower layer may depend back on it.

### `apps/palette`: desktop app composition

`apps/palette` remains the actual desktop application. It owns the Vite/React source, Tauri app package, `tauri.conf.json`, icons, capabilities, installer/package metadata, and app-local command registration.

The target Tauri package is still app-local:

```text
apps/palette/src-tauri
    depends on soma-tauri-shell for reusable desktop shell mechanics
    depends on soma-palette when it needs shared product DTOs or product route helpers
    supplies product name, app identifier, icons, window defaults, and command wiring
```

Do not move the whole desktop app into `crates/`. Tauri packaging expects an application boundary with frontend assets and bundle metadata. Extract only reusable Rust mechanics or product server/API contracts.

Keep `apps/palette/src-tauri` as an app-local Tauri workspace/package by default, even when it depends on root workspace crates by path. Add it to the root Cargo workspace only if the build and release tooling benefit more than the Tauri-local lockfile/package boundary does.

---

## 3.2 `crates/shared/traces`: RMCP trace-context helpers

**Package:** `rmcp-traces`

Suggested layout:

```text
crates/shared/traces/src/
├── lib.rs
├── traceparent.rs
├── tracestate.rs
├── baggage.rs
├── metadata.rs
├── limits.rs
├── redaction.rs
└── error.rs
```

### Owns

- bounded extraction of trace metadata from RMCP request metadata
- W3C `traceparent`, `tracestate`, and baggage parsing
- validation and limits
- redacted, log-safe summaries
- transport-neutral trace context values where appropriate

### Does not own

- Soma request policy
- application authorization
- gateway routing
- OpenTelemetry exporter setup
- HTTP middleware
- MCP server lifecycle

### Dependencies

Prefer only external dependencies. `soma-mcp-server` may depend on `rmcp-traces`; the reverse is forbidden.

---

## 3.3 `crates/shared/auth`: reusable auth implementation

**Package:** `soma-auth` during the migration.

The live crate currently has no dependency on Soma product crates. It is structurally reusable and belongs in the shared layer.

Suggested layout remains close to its current shape:

```text
crates/shared/auth/src/
├── lib.rs
├── config.rs
├── auth_context.rs
├── middleware.rs
├── routes.rs
├── jwt.rs
├── session.rs
├── sqlite.rs
├── google.rs
├── registration.rs
├── metadata.rs
├── cimd/
└── upstream/
```

### Owns

- reusable bearer-token, OAuth, JWT, session, and SQLite-backed auth primitives
- optional Axum middleware and auth route builders
- configurable scope labels and static-token scope minting
- reusable inbound MCP/HTTP server authorization primitives so products can expose
  MCP servers protected by bearer tokens or OAuth without embedding product defaults
- upstream OAuth credential storage, cache, refresh, and manager support
- generic authorization-server and protected-resource metadata helpers
- token encryption and key management primitives

### Does not own

- Soma product scopes, principals, or admin policy
- `SOMA_*` environment loading
- Soma default paths, cookie names, or data directories
- gateway route policy
- product audit policy
- application authorization ports

### Publishability cleanup

Before publishing or declaring this crate shared-stable:

- replace `~/.soma` default data directory with a neutral default or require the consumer to supply one
- replace legacy `LAB` env prefix defaults with explicit consumer configuration
- replace `lab_session`, `lab`, `lab:read`, and `lab:admin` defaults with neutral or required consumer values
- move Soma/Lab-specific defaults into `crates/soma/config` or `crates/soma/integrations/auth.rs`
- update package description and docs so they describe a reusable auth crate, not "for soma and derived servers"

The package name may remain `soma-auth` temporarily to reduce churn. Pick a brand-neutral publish name only when the crate is ready for external publishing.

---

## 3.4 `crates/shared/observability`: reusable observability helpers

**Package:** `soma-observability` during the migration.

The live crate currently has no dependency on Soma product crates. It is reusable after removing product-specific default strings and env names.

Suggested layout remains close to its current shape:

```text
crates/shared/observability/src/
├── lib.rs
├── metrics.rs
├── logging.rs
├── logging/
│   ├── aurora.rs
│   └── formatter.rs
└── binary_status.rs
```

### Owns

- Prometheus recorder install/render helpers
- reusable tracing/logging initialization primitives
- reusable structured/colored log formatter
- reusable terminal/log palette constants where not product-specific
- configurable stale-binary/source freshness helper

### Does not own

- Soma metric names and dashboards
- product audit/telemetry policy
- hard-coded `SOMA_*` env vars
- hard-coded `soma` binary names, rebuild commands, or product paths
- product readiness aggregation

### Publishability cleanup

Before publishing or declaring this crate shared-stable:

- parameterize `SOMA_SUPPRESS_STALE_BINARY_WARNING`
- parameterize the stale-binary warning text, binary name, build command, and source inputs
- remove or generalize docs that say "When adapting Soma"
- ensure logging palette defaults are either neutral shared defaults or explicitly branded as Aurora reusable tokens

Product metric naming and dashboards belong in `crates/soma/integrations` or `crates/soma/runtime`, not in `crates/shared/observability`.

---

## 3.5 `crates/shared/openapi`: reusable OpenAPI engine

**Package:** `soma-openapi`

Suggested layout:

```text
crates/shared/openapi/src/
├── lib.rs
├── config.rs
├── document.rs
├── registry.rs
├── operation.rs
├── request.rs
├── dispatch.rs
├── auth.rs
├── ssrf.rs
├── schema.rs
├── response.rs
└── error.rs
```

### Owns

- OpenAPI document loading and parsing
- operation indexing
- reusable operation selection
- generic request construction
- HTTP execution
- SSRF and network-destination policy primitives
- generic credential injection hooks
- generic result and error types
- explicit configuration independent of Soma files and environment

### Does not own

- Soma action names
- Soma scopes
- Soma destructive confirmation
- Soma provider manifests
- CLI, API, or MCP presentation
- product audit policy

### Integration direction

```text
soma-integrations::openapi
    implements a soma-application port
    delegates execution to soma-openapi
```

A reusable provider adapter may also project indexed OpenAPI operations into `soma-provider-core`:

```text
soma-provider-adapters::openapi
    depends on soma-openapi + soma-provider-core
```

Do not create a second OpenAPI executor inside `provider-adapters`, `soma-application`, or `soma-gateway`.

---

## 3.6 `crates/shared/codemode`: reusable Code Mode runtime

**Package:** `soma-codemode`

Suggested layout:

```text
crates/shared/codemode/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs
│   ├── protocol.rs
│   ├── runner.rs
│   ├── pool.rs
│   ├── execution.rs
│   ├── artifact.rs
│   ├── state.rs
│   ├── workspace.rs
│   ├── git.rs
│   ├── limits.rs
│   ├── openapi.rs
│   └── error.rs
└── src/bin/
    └── soma-codemode-runner.rs
```

### Owns

- Code Mode runner process protocol
- runner lifecycle and pooling
- execution requests/results
- artifacts
- workspace and state management
- process limits and cancellation
- generic Git integration
- optional integration with `soma-openapi`
- explicit configuration usable outside Soma

### Does not own

- Soma authorization
- Soma action dispatch
- Soma response limits
- Soma provider catalog policy
- Soma CLI presentation
- product-specific paths as mandatory construction inputs

### Product integration

```text
soma-application::CodeModeExecutor port
              ▲
              │ implements
soma-integrations::CodeModeAdapter
              │
              ▼
soma-codemode
```

The product adapter may map `SOMA_HOME`, product auth context, audit fields, and response policy into the standalone engine. The engine itself should not require those concepts.

---

## 3.7 `crates/shared/mcp/gateway`: reusable upstream MCP gateway

**Package:** `soma-gateway`

Suggested layout:

```text
crates/shared/mcp/gateway/src/
├── lib.rs
├── config.rs
├── manager.rs
├── catalog.rs
├── route_policy.rs
├── protected_route.rs
├── virtual_route.rs
├── config_store.rs
├── auth.rs
├── credentials.rs
├── projection.rs
├── codemode.rs
├── openapi.rs
├── palette.rs
├── usage.rs
├── reload.rs
├── trace.rs
└── error.rs
```

### Owns

- generic gateway composition over `soma-mcp-client`, `soma-mcp-server`, and `soma-mcp-proxy`
- upstream registry and gateway configuration model
- route and tool-name mapping policy
- catalog aggregation
- upstream health and reload primitives
- protected-route primitives
- virtual-route primitives
- generic auth, authorization, and credential hooks
- generic trace propagation
- gateway administration primitives
- Code Mode, OpenAPI, and Palette projection adapters

### Does not own

- product auth types or product auth defaults
- Soma scopes or principals
- Soma REST routes
- Soma MCP tool descriptions
- Soma URI schemes such as `soma://upstream/`
- `SOMA_HOME`, `.soma`, or other Soma configuration defaults
- product-level confirmation policy
- product audit policy
- product configuration file layout

### Depends on

Target:

```text
soma-gateway ─────────────▶ soma-mcp-client
soma-gateway ─────────────▶ soma-mcp-server
soma-gateway ─────────────▶ soma-mcp-proxy
soma-mcp-proxy ───────────▶ soma-mcp-client
soma-mcp-proxy ───────────▶ soma-mcp-server
```

The gateway composes the role crates directly; it does not reach the client only
through the proxy. This matches the shared-layer DAG in section 4.3.

Live as of 2026-07-16, the `soma-mcp-server` edges do not exist yet:

```text
soma-gateway ─────────────▶ soma-mcp-client      present
soma-gateway ─────────────▶ soma-mcp-proxy       present
soma-mcp-proxy ───────────▶ soma-mcp-client      present
```

Both crates still reach for `rmcp` server types directly instead of going through
`soma-mcp-server`. PR 14 closes that gap when it moves the residual inbound
lifecycle, paging, and protocol mechanics into the server role crate. Until then
the two diagrams above differ on purpose — do not "fix" section 4.3 to match the
manifests.

The gateway is the reusable engine that users instantiate when they want a full MCP aggregation runtime. It is not the primitive client or server library. A project that only needs to call upstream MCP servers depends on `soma-mcp-client`; a project that only needs to expose an MCP server depends on `soma-mcp-server`; a project that needs to bridge inbound MCP requests to upstream servers depends on `soma-mcp-proxy`.

### Critical boundary

This dependency is forbidden:

```text
crates/shared/mcp/gateway ──X──▶ crates/soma/*
```

An optional dependency on `crates/shared/auth` is allowed only if it remains generic and does not pull in Soma product defaults. Product mapping belongs here:

```text
crates/soma/integrations/src/gateway_auth.rs
```

That bridge can depend on both `soma-gateway` and shared `soma-auth` and implement the generic gateway auth hook with Soma product defaults.

The reviewed gateway branch currently has these Soma-shaped pieces that must be neutralized before `soma-gateway` is declared shared:

- `soma-auth` optional dependency currently points at a product-path crate; retarget it to `crates/shared/auth` or hide it behind a generic trait.
- `SOMA_HOME`, `.soma`, and `soma_home` validation in gateway path defaults.
- default protected-route scopes of `soma:read` and `soma:write`.
- `soma.gateway.error.v1` as the structured error schema namespace.
- `soma://upstream/` as the upstream resource URI prefix.
- special-case routing for a native tool named `soma`.
- error/remediation text that says "start Soma" instead of naming the host application.

Replace these with explicit gateway configuration, generic policy traits, or product adapters under `crates/soma/integrations`. If a separate product wrapper becomes useful, create it under `crates/soma/gateway`; do not let product defaults live in `crates/shared/mcp/gateway`.

### Relationship to Code Mode and OpenAPI

The gateway may have optional dependencies on `soma-codemode` and `soma-openapi` because all three are shared crates. The gateway must delegate to those engines instead of carrying duplicate implementations.

Valid:

```text
soma-gateway --features codemode ──▶ soma-codemode
soma-gateway --features openapi  ──▶ soma-openapi
```

Invalid:

```text
soma-gateway
    contains an independent Code Mode runner implementation
    contains an independent OpenAPI HTTP execution implementation
```

Keep gateway-owned code limited to routing, projection, and gateway-specific adaptation. The reviewed gateway branch's Code Mode, OpenAPI, and Palette modules are acceptable only where they are adapter/projection code; reusable runners, catalogs, HTTP execution, and schema engines belong in `crates/shared/codemode`, `crates/shared/openapi`, or their provider adapters.

---

## 3.8 `crates/shared/provider-core`: reusable provider framework

**Package:** `soma-provider-core`

This crate owns the shared provider contract. Providers feed into `ProviderCatalog`, and each executable operation is described by a `ToolSpec`.

The reusable mental model is:

```text
provider implementation
    emits ProviderCatalog
        contains ToolSpec entries
            surfaces project into MCP tools, REST routes, CLI commands, Palette actions
```

`ToolSpec` is the canonical shared type. The current `ActionSpec` shape in `soma-contracts` is a product/static-action precursor that should be migrated into this model. Soma's concrete built-in list may remain named `ACTION_SPECS` or become `SOMA_ACTIONS`, but it should be product-owned and adapted into provider-core `ToolSpec` entries.

`ProviderTool` may remain as a temporary compatibility alias:

```rust
pub type ProviderTool = ToolSpec;
```

The alias exists only to reduce migration churn for existing imports, generated docs, tests, and provider manifests while the architecture moves to the clearer `ToolSpec` name. It should not become a second semantic type.

Suggested layout:

```text
crates/shared/provider-core/src/
├── lib.rs
├── id.rs
├── manifest.rs
├── schema.rs
├── validation.rs
├── capability.rs
├── tool.rs
├── action.rs       # optional builders/aliases for one-action-dispatch ergonomics
├── prompt.rs
├── resource.rs
├── task.rs
├── elicitation.rs
├── call.rs
├── output.rs
├── provider.rs
├── registry/
│   ├── mod.rs
│   ├── builder.rs
│   ├── index.rs
│   ├── snapshot.rs
│   ├── fingerprint.rs
│   └── dispatch.rs
├── surface.rs
├── limits.rs
└── error.rs
```

### Owns

- generic provider manifest model
- provider schema validation
- capabilities and grants as generic provider concepts
- provider trait
- `ToolSpec` as the canonical executable operation metadata
- optional `ProviderTool` compatibility alias and `ActionSpec` builder/alias when useful for one-action-dispatch ergonomics
- prompt, resource, task, and elicitation metadata
- provider registration
- immutable snapshots and fingerprints
- indexes for action names and surface overlays
- generic dispatch
- generic provider errors

### Does not own

- Soma authorization policy
- Soma configuration
- Soma built-in commands
- Soma's concrete `ACTION_SPECS` / `SOMA_ACTIONS` list
- `SomaAction` product enum or product request parsing
- transport-specific HTTP, MCP, or CLI DTOs
- process startup
- a concrete OpenAPI engine
- a concrete Code Mode engine
- an upstream MCP gateway implementation

### Relationship to gateway

These are distinct bounded contexts:

```text
soma-provider-core
    In-process provider capability registry and projection model.

soma-gateway
    Upstream MCP topology, connections, routing, sessions, and lifecycle.
```

A gateway adapter can project gateway capabilities into the provider registry, but the registries should not be fused.

---

## 3.9 `crates/shared/provider-adapters`: reusable provider implementations

**Package:** `soma-provider-adapters`

Suggested layout:

```text
crates/shared/provider-adapters/src/
├── lib.rs
├── static_rust.rs
├── manifest_file.rs
├── typescript.rs
├── python.rs
├── wasm.rs
├── ai_sdk.rs
├── openapi.rs
├── codemode.rs
├── gateway.rs
└── error.rs
```

### Owns

Reusable implementations of `soma-provider-core` contracts, including feature-gated bridges to other shared engines.

Examples:

```text
openapi adapter
    soma-provider-core + soma-openapi

codemode adapter
    soma-provider-core + soma-codemode

gateway adapter
    soma-provider-core + soma-gateway

WASM/Python/TypeScript adapters
    soma-provider-core + their generic runtimes
```

### Product-specific exception

A provider that calls back into Soma's own product API, uses Soma auth types, or depends on Soma runtime state belongs in `crates/soma/integrations`, not here.

### Do not over-split

Start with one feature-gated `provider-adapters` crate. Create one crate per adapter only when independent versioning, dependency weight, or ownership makes the split valuable.

---

## 3.10 MCP role crates: client, server, and proxy

Split reusable MCP infrastructure by protocol role. This keeps each consumer's dependency surface honest:

```text
Want to call upstream MCP servers?        depend on soma-mcp-client
Want to expose an MCP server?             depend on soma-mcp-server
Want to bridge inbound to upstream MCP?   depend on soma-mcp-proxy
Want the full aggregation gateway?        depend on soma-gateway
```

The role crates are shared infrastructure. None may import Soma product crates, Soma scopes, Soma config defaults, or Soma tool schemas.

### `crates/shared/mcp/client`

**Package:** `soma-mcp-client`

Suggested layout:

```text
crates/shared/mcp/client/src/
├── lib.rs
├── config.rs
├── session.rs
├── pool.rs
├── discovery.rs
├── tools.rs
├── resources.rs
├── prompts.rs
├── health.rs
├── transport/
│   ├── mod.rs
│   ├── stdio.rs
│   ├── streamable_http.rs
│   ├── sse.rs
│   └── websocket.rs
├── process.rs
├── security.rs
└── error.rs
```

Owns outbound MCP client sessions, upstream discovery, tool/resource/prompt calls, stdio process lifecycle, HTTP/SSE/WebSocket client transports, response caps, upstream health, bearer-token attachment for upstreams configured with explicit token env vars, OAuth provider hooks for upstream MCP servers secured by OAuth, and client-side security checks such as SSRF and environment redaction.

Does not own inbound `ServerHandler` implementations, route aggregation, protected public routes, gateway administration, or a concrete product auth implementation. OAuth support is a generic provider/manager seam; adapters such as `soma-auth` live in `crates/soma/integrations` or another product integration crate.

### `crates/shared/mcp/server`

**Package:** `soma-mcp-server`

Suggested layout:

```text
crates/shared/mcp/server/src/
├── lib.rs
├── stdio.rs
├── http.rs
├── lifecycle.rs
├── cancellation.rs
├── paging.rs
├── protocol.rs
├── error_result.rs
├── conformance.rs
└── trace.rs
```

Owns reusable inbound RMCP server lifecycle helpers, stdio and HTTP serving helpers, cancellation/shutdown integration, response page storage, protocol conversion helpers, conformance-test helpers, auth hook points for bearer/OAuth-protected MCP servers, and integration with `rmcp-traces`.

Does not own Soma tools, prompts, resources, scopes, product action dispatch, product-specific MCP error messages, or concrete auth defaults. Products can pair it with `soma-auth` or a custom authorizer to expose OAuth-protected MCP servers.

### `crates/shared/mcp/proxy`

**Package:** `soma-mcp-proxy`

Suggested layout:

```text
crates/shared/mcp/proxy/src/
├── lib.rs
├── routes.rs
├── catalog.rs
├── call.rs
├── resources.rs
├── prompts.rs
├── naming.rs
├── subject.rs
├── policy.rs
└── error.rs
```

Owns generic bridging from inbound MCP surfaces to outbound upstream MCP clients: route naming, collision handling, catalog projection, subject propagation, protected subset filtering, and proxy error mapping.

Does not own upstream transport implementations, gateway config storage, product scopes, product auth types, or product URI schemes. URI prefixes such as `soma://upstream/` must be supplied by the host product or gateway configuration.

### Scope warning

These crates should remain thin wrappers around RMCP. RMCP already supplies protocol primitives. Extract only behavior that Soma, the shared gateway, and another unrelated project can genuinely share.

### MCP tool exposure mode

MCP should support both presentation styles over the same `ToolSpec` and provider dispatch path:

```rust
pub enum McpToolMode {
    Router,
    Individual,
    Both,
}
```

`Router` is the default Soma mode and preserves the current one-tool dispatch pattern:

```text
tool: soma
args: { "action": "echo", "message": "hello" }
```

`Individual` exposes one MCP tool per `ToolSpec`:

```text
tool: echo
args: { "message": "hello" }
```

`Both` is useful for migrations, compatibility testing, and clients that want to compare schemas.

This mode is an MCP adapter concern only. It must not create duplicate business logic. In every mode, the adapter resolves a tool call to the same provider action name and dispatches the same `ToolSpec` through the same application/provider path.

### Surface projection rule

The same operation is implemented once and projected into each surface:

```text
ToolSpec
    ├── MCP router action or individual MCP tool
    ├── REST route metadata, consumed by product API routes
    ├── CLI command metadata, defaulting command names from the action/tool name
    └── Palette/UI action metadata when enabled

CLI/API/MCP shims
    parse protocol-specific input
    resolve action/tool name
    call the same application/provider operation
    format protocol-specific output
```

REST remains traditional typed endpoints such as `POST /v1/echo` and `GET /v1/status`, not an action-envelope API. CLI commands are named from `ToolSpec.cli.command` when set, otherwise from the tool/action name.

---

## 3.11 `crates/shared/http-api`: reusable HTTP API surface helpers

**Package:** `soma-http-api`

`http-server` owns server lifecycle. `http-api` owns reusable API surface mechanics.

Suggested layout:

```text
crates/shared/http-api/src/
├── lib.rs
├── response.rs
├── error.rs
├── problem.rs
├── probe.rs
├── route_inventory.rs
├── pagination.rs
├── json.rs
├── openapi.rs
└── axum.rs
```

### Owns

- reusable JSON response envelopes and error body helpers
- reusable problem-details or structured-error response helpers
- liveness/readiness probe DTOs and response helpers
- route inventory metadata and documentation helpers
- generic pagination/query DTO helpers
- optional Axum `IntoResponse` adapters
- optional OpenAPI metadata projection helpers that delegate to `soma-openapi`

### Does not own

- `/v1/*` Soma routes
- Soma action names or REST paths
- product auth policy
- product service/runtime state
- listener binding or graceful shutdown
- embedded web UI assets

### Relationship to `http-server`

```text
soma-api
    product routes and request translation

soma-http-api
    reusable API response/error/probe/route-inventory helpers

soma-http-server
    listener, middleware, CORS, static files, shutdown, SSE/WebSocket helpers
```

Do not use `http-server` as a drawer for reusable API contracts. If a helper is about JSON API shape, route metadata, or HTTP error/probe DTOs, it belongs in `http-api`. If it is about running an Axum service, request middleware, or transport lifecycle, it belongs in `http-server`.

---

## 3.12 `crates/shared/http-server`: reusable HTTP server plumbing

**Package:** `soma-http-server`

Suggested layout:

```text
crates/shared/http-server/src/
├── lib.rs
├── server.rs
├── config.rs
├── shutdown.rs
├── middleware/
│   ├── mod.rs
│   ├── request_id.rs
│   ├── tracing.rs
│   ├── timeout.rs
│   ├── body_limit.rs
│   └── cors.rs
├── health.rs
├── rejection.rs
├── error.rs
├── pagination.rs
├── sse.rs
├── websocket.rs
└── static_files.rs
```

### Owns

- listener binding and Axum server lifecycle
- graceful shutdown plumbing
- request IDs
- generic request tracing
- timeouts and body limits
- generic CORS configuration
- reusable SSE, WebSocket, and static-file helpers when proven reusable

### Does not own

- `/v1/*` Soma routes
- product auth policy
- Soma OpenAPI document content
- embedded Soma UI assets
- action dispatch

---

## 3.13 `crates/shared/cli-core`: reusable CLI plumbing

**Package:** `soma-cli-core`

Suggested layout:

```text
crates/shared/cli-core/src/
├── lib.rs
├── common_args.rs
├── output.rs
├── table.rs
├── json.rs
├── confirmation.rs
├── terminal.rs
├── color.rs
├── progress.rs
├── completion.rs
└── error.rs
```

### Owns

- common verbosity arguments
- output-format selection
- terminal and JSON rendering helpers
- table rendering
- confirmation I/O primitives
- terminal/color capability policy
- progress helpers
- shell completion helpers
- reusable CLI error presentation

### Does not own

- `soma gateway reload`
- `soma status`
- `soma doctor`
- Soma action names
- product defaults or scopes
- business confirmation policy

The CLI adapter may ask a human for confirmation. The application layer must still verify that a destructive operation is authorized and confirmed. A non-CLI surface cannot be allowed to bypass the rule.

---

## 3.14 `crates/shared/tauri-shell`: reusable Tauri desktop shell helpers

**Package:** `soma-tauri-shell`

This crate owns reusable Rust mechanics for Tauri desktop shells. It is intentionally named `tauri-shell`, not `palette`, because the reusable API is desktop-window/Tauri behavior rather than Soma's command-palette product.

Suggested layout:

```text
crates/shared/tauri-shell/src/
├── lib.rs
├── app.rs
├── window.rs
├── tray.rs
├── shortcut.rs
├── blur.rs
├── persistence.rs
├── command.rs
├── oauth_window.rs
└── error.rs
```

### Owns

- Tauri app/window show, hide, focus, resize, center, and shadow helpers
- tray icon setup helpers
- global shortcut parsing, registration, rebind, and active-shortcut tracking
- blur-dismiss state and window-event helpers
- generic app-data path and JSON persistence helpers
- command result/error helpers for Tauri command handlers
- optional product-neutral browser-open or loopback callback helpers when fully configured by the caller

### Does not own

- Soma or Labby settings types
- `LABBY_*` or `SOMA_*` environment defaults
- `/v1/palette/*` HTTP calls
- product OAuth policy or server discovery defaults
- app identifier, product name, icons, capabilities, or `tauri.conf.json`
- frontend React components or CSS
- provider `ToolSpec` / Palette overlay contracts

`crates/shared/provider-core` owns generic Palette surface metadata through `ToolSpec` overlays. `crates/shared/tauri-shell` owns the native desktop shell mechanics. Do not create `crates/shared/palette` unless a third, product-neutral palette domain emerges that is neither provider metadata nor Tauri shell behavior.

---

## 3.15 `crates/shared/codex-app-server-client`

**Package:** `codex-app-server-client`

Keep the current standalone client here.

### Owns

- the typed Codex app-server protocol client
- transport and protocol DTOs specific to that external server
- no Soma product behavior

It may later be consumed by Code Mode, gateway, provider adapters, or Soma integrations depending on the actual use case. Do not move it into `crates/soma` merely because Soma currently consumes it.

---

## 3.16 `crates/soma/domain`: product concepts and invariant rules

**Package:** `soma-domain`

Suggested layout:

```text
crates/soma/domain/src/
├── lib.rs
├── action.rs
├── provider.rs
├── execution.rs
├── principal.rs
├── scope.rs
├── confirmation.rs
├── scaffold.rs
├── policy.rs
└── error.rs
```

### Owns

- Soma-specific value objects
- Soma-specific invariant rules
- state transitions that must remain valid regardless of transport
- product concepts with behavior, not passive DTO buckets

Examples:

- whether a Soma operation requires destructive confirmation
- valid Soma scope combinations
- product rules for protected capabilities
- scaffold intent invariants
- product-level execution classifications

### Does not own

- Axum, Clap, or RMCP types
- Reqwest clients
- configuration loading
- provider JSON schemas merely because they are shared
- application workflow orchestration
- database or filesystem implementations

### Keep it small

Do not move all of `soma-contracts` into `soma-domain`. Configuration, provider manifests, API DTOs, MCP DTOs, and generic schemas belong elsewhere.

---

## 3.17 `crates/soma/application`: shared product use cases

**Package:** `soma-application`

Suggested layout:

```text
crates/soma/application/src/
├── lib.rs
├── app.rs
├── context.rs
├── error.rs
├── ports/
│   ├── mod.rs
│   ├── provider_runtime.rs
│   ├── upstream.rs
│   ├── gateway.rs
│   ├── codemode.rs
│   ├── openapi.rs
│   ├── authorizer.rs
│   ├── audit.rs
│   └── clock.rs
├── actions/
│   ├── mod.rs
│   ├── execute.rs
│   ├── catalog.rs
│   └── inspect.rs
├── providers/
│   ├── mod.rs
│   ├── refresh.rs
│   └── snapshot.rs
├── gateway/
│   ├── mod.rs
│   ├── status.rs
│   ├── reload.rs
│   └── execute.rs
├── codemode/
│   ├── mod.rs
│   └── execute.rs
├── openapi/
│   ├── mod.rs
│   └── execute.rs
├── prompts/
├── resources/
├── status/
├── doctor/
└── scaffold/
```

### Owns

- every business operation shared by CLI, REST, and MCP
- workflow orchestration
- product authorization checks
- destructive-action policy
- product defaults
- local-versus-remote application policy
- audit intent
- response-cap policy
- application errors
- ports required from infrastructure and shared engines

### Does not own

- transport parsing or output rendering
- Axum status codes
- MCP tool schemas
- Clap command definitions
- Reqwest implementation details
- gateway process/network implementation
- Code Mode runner internals
- OpenAPI HTTP implementation

### Suggested facade

```rust
pub struct SomaApplication {
    actions: ActionUseCases,
    providers: ProviderUseCases,
    gateway: GatewayUseCases,
    codemode: CodeModeUseCases,
    openapi: OpenApiUseCases,
    resources: ResourceUseCases,
    prompts: PromptUseCases,
    status: StatusUseCases,
}
```

Surface crates receive:

```rust
Arc<SomaApplication>
```

They do not receive `SomaClient`, `SomaService`, `ProviderRegistry`, `soma-gateway`, or `soma-openapi` directly.

### Execution context

Use one transport-neutral context:

```rust
pub struct ExecutionContext {
    pub principal: Option<Principal>,
    pub surface: Surface,
    pub trace: Option<TraceContext>,
    pub destructive_confirmation: Confirmation,
    pub response_limit: Option<usize>,
    pub request_id: RequestId,
}
```

Transport adapters construct it. Application use cases enforce it.

### Example shared operation

```rust
pub async fn execute_action(
    &self,
    request: ExecuteActionRequest,
    context: ExecutionContext,
) -> Result<ExecuteActionResponse, ApplicationError> {
    self.authorizer.authorize(&context, &request).await?;
    self.confirmation_policy.check(&context, &request)?;
    let output = self.provider_runtime.execute(request).await?;
    self.response_policy.apply(output, context.response_limit)
}
```

The API endpoint, MCP tool, and CLI command all invoke this same method.

---

## 3.18 `crates/soma/config`: product configuration

**Package:** `soma-config`

Suggested layout:

```text
crates/soma/config/src/
├── lib.rs
├── load.rs
├── env.rs
├── file.rs
├── defaults.rs
├── paths.rs
├── validation.rs
├── feature.rs
└── error.rs
```

### Owns

- Soma environment variables
- Soma configuration file format
- product defaults
- product path resolution
- configuration validation
- mapping user-facing product configuration into explicit engine configs

### Does not own

- engine configuration types themselves
- business workflows
- process startup
- secrets as tool-call arguments

Example:

```text
SomaConfig.gateway
    maps into soma_gateway::GatewayConfig

SomaConfig.codemode
    maps into soma_codemode::CodeModeConfig

SomaConfig.openapi
    maps into soma_openapi::OpenApiConfig
```

The standalone crates keep their own explicit config types. `soma-config` performs product mapping.

---

## 3.19 `crates/soma/client`: Soma upstream client

**Package:** `soma-client`

Suggested layout:

```text
crates/soma/client/src/
├── lib.rs
├── client.rs
├── config.rs
├── request.rs
├── response.rs
├── transport.rs
└── error.rs
```

### Owns

- concrete outbound transport for a remote Soma server
- HTTP request construction
- remote response decoding
- transport retries and timeouts when they are client concerns
- implementation of an application-owned upstream port

### Does not own

- local/remote selection policy
- CLI parsing
- product action validation
- provider registry construction

The application layer decides when a request should use an upstream. The client only performs the transport.

---

## 3.20 `crates/soma/integrations`: product adapters to shared engines

**Package:** `soma-integrations`

Suggested layout:

```text
crates/soma/integrations/src/
├── lib.rs
├── provider_runtime.rs
├── gateway.rs
├── gateway_auth.rs
├── gateway_trace.rs
├── auth.rs
├── observability.rs
├── codemode.rs
├── openapi.rs
├── upstream.rs
├── remote_provider.rs
├── audit.rs
└── error.rs
```

This crate answers the earlier adapter question precisely. Product defaults and adapters connecting the standalone gateway to shared auth go here, not under `apps/soma` and not inside `soma-gateway`.

### Owns

- implementations of `soma-application` ports
- translation between Soma product types and shared-engine types
- gateway-to-auth bridge and product auth default mapping
- gateway trace propagation bridge
- observability setup using Soma product names, env vars, and dashboards
- provider registry adapter
- Code Mode adapter
- OpenAPI adapter
- remote Soma provider implementation
- product-specific integration error translation

### Does not own

- CLI, HTTP, or MCP DTOs
- product workflow ordering
- engine internals
- process entry points

### Dependency shape

```text
soma-integrations
├── soma-application
├── soma-domain
├── soma-auth
├── soma-observability
├── soma-client
├── soma-provider-core
├── soma-provider-adapters
├── soma-gateway
├── soma-codemode
└── soma-openapi
```

This is intentionally an outer-layer crate. It is allowed to see both application ports and concrete engines.

### Why one integration crate first

Do not immediately create:

```text
soma-gateway-integration
soma-codemode-integration
soma-openapi-integration
soma-provider-integration
```

Start with one `soma-integrations` crate and modules. Split only when compile weight, independent ownership, or reuse across multiple products justifies it.

---

## 3.21 `crates/soma/runtime`: initialized product runtime

**Package:** `soma-runtime`

Suggested layout:

```text
crates/soma/runtime/src/
├── lib.rs
├── runtime.rs
├── handles.rs
├── supervisor.rs
├── background.rs
├── readiness.rs
├── shutdown.rs
└── error.rs
```

### Owns

- initialized `Arc<SomaApplication>` handle
- long-running product task handles
- task supervision
- refresh loops and background jobs
- readiness aggregation
- coordinated runtime shutdown

### Does not own

- CLI parsing
- Axum routing
- RMCP tool definitions
- concrete business operations
- raw `SomaService` and `ProviderRegistry` exposure

Target runtime state:

```rust
pub struct SomaRuntime {
    application: Arc<SomaApplication>,
    supervisor: TaskSupervisor,
    readiness: ReadinessHandle,
}
```

Surface state should expose the application facade, not every lower-level dependency.

---

## 3.22 `crates/soma/api`: Soma HTTP adapter

**Package:** `soma-api`

Suggested layout:

```text
crates/soma/api/src/
├── lib.rs
├── router.rs
├── state.rs
├── error.rs
├── dto/
├── routes/
│   ├── actions.rs
│   ├── providers.rs
│   ├── gateway.rs
│   ├── codemode.rs
│   ├── openapi.rs
│   ├── prompts.rs
│   ├── resources.rs
│   ├── status.rs
│   └── doctor.rs
└── openapi.rs
```

### Owns

- Soma paths and HTTP methods
- Axum extractors
- request and response DTOs
- HTTP status-code mapping
- API-specific pagination representation
- product OpenAPI document generation
- translation into `soma-application` requests

### Uses

- `soma-http-api` for reusable JSON response, error, probe, pagination, and route-inventory helpers
- `soma-http-server` only through app/server composition for listener and middleware lifecycle

### Does not own

- action execution workflows
- direct provider registry dispatch
- construction of `SomaClient` or shared engines
- listener binding
- generic middleware
- reusable health/readiness DTOs
- generic error envelopes
- reusable route inventory primitives

Target state:

```rust
pub struct ApiState {
    pub application: Arc<SomaApplication>,
}
```

---

## 3.23 `crates/soma/mcp`: Soma MCP adapter

**Package:** `soma-mcp`

Suggested layout:

```text
crates/soma/mcp/src/
├── lib.rs
├── server.rs
├── state.rs
├── error.rs
├── tools/
│   ├── mod.rs
│   ├── soma.rs
│   ├── gateway.rs
│   ├── codemode.rs
│   └── openapi.rs
├── prompts/
├── resources/
├── schemas/
└── mapping.rs
```

### Owns

- Soma MCP tool names and descriptions
- MCP input/output schemas
- prompts and resources exposed by Soma
- RMCP request metadata translation
- MCP protocol error mapping
- translation into `soma-application` requests

### Uses

- `soma-mcp-server` for reusable inbound lifecycle, transport, paging, and protocol helpers
- `soma-mcp-proxy` only when the Soma MCP adapter exposes gateway-proxied upstream tools/resources/prompts
- `rmcp-traces` through `soma-mcp-server` or directly for bounded trace extraction

### Does not own

- direct provider dispatch
- gateway lifecycle
- business confirmation policy
- application response policy
- process startup

Target state:

```rust
pub struct McpState {
    pub application: Arc<SomaApplication>,
}
```

---

## 3.24 `crates/soma/cli`: Soma CLI adapter

**Package:** `soma-cli`

Suggested layout:

```text
crates/soma/cli/src/
├── lib.rs
├── parser.rs
├── invocation.rs
├── error.rs
├── output.rs
└── commands/
    ├── actions.rs
    ├── providers.rs
    ├── gateway.rs
    ├── codemode.rs
    ├── openapi.rs
    ├── status.rs
    ├── doctor.rs
    ├── setup.rs
    └── scaffold.rs
```

### Owns

- Soma command names and arguments
- CLI parsing
- interactive confirmation prompt
- terminal and JSON output mapping
- exit-code mapping
- translation into `soma-application` requests

### Uses

- `soma-cli-core` for generic output, terminal, confirmation, and completion helpers

### Does not own

- direct construction of `SomaClient`, `SomaService`, or provider registry
- provider refresh policy
- local/remote business behavior
- action execution

The CLI may collect confirmation, but the application validates the confirmation requirement.

---

## 3.25 `crates/soma/palette`: Soma Palette product API and adapter

**Package:** `soma-palette`

This crate owns Soma-specific Palette behavior that is shared between the HTTP server and the desktop app, especially server-side Palette routes and product DTOs. It is product code, not a reusable desktop shell.

Suggested layout:

```text
crates/soma/palette/src/
├── lib.rs
├── router.rs
├── state.rs
├── dto.rs
├── catalog.rs
├── execute.rs
├── schema.rs
├── auth.rs
├── launcher.rs
└── error.rs
```

### Owns

- `/v1/palette/*` product routes and handlers
- Palette DTOs used by Soma's HTTP API and desktop app
- product mapping from provider `ToolSpec` / Palette overlays into Palette actions
- product launcher catalog and execution policy
- product auth/session behavior for Palette requests
- product error mapping for Palette UI responses
- product OpenAPI route metadata for Palette endpoints

### Does not own

- Tauri app/window/tray/shortcut mechanics
- `tauri.conf.json`, icons, capabilities, bundle metadata, or installers
- React components, CSS, or frontend state
- generic provider `ToolSpec` definitions
- generic Palette overlay contracts
- generic HTTP API response helpers

### Relationship to app and shared crates

```text
apps/palette
    desktop frontend and Tauri app composition

apps/palette/src-tauri
    app-local native package; wires Tauri commands to product APIs

crates/shared/tauri-shell
    reusable Tauri shell mechanics

crates/soma/palette
    Soma Palette server routes, DTOs, product mapping, and product policy
```

Do not create `crates/shared/palette` yet. Generic Palette action metadata belongs in `soma-provider-core` as `ToolSpec` overlays, and reusable native shell behavior belongs in `soma-tauri-shell`.

---

## 3.26 Remaining Soma product crates

### `crates/soma/test-support`

The live crate is not a full harness today; it only provides shared tracing-capture helpers used by a few tests.

Recommended target:

- keep a standalone `soma-test-support` crate only if it grows into product fixtures, fake application ports, contract snapshot helpers, process fixtures, or test configuration
- otherwise fold the tracing helper into the tests that use it, or move a product-neutral version into a future shared testing helper only when multiple unrelated projects need it
- do not let `soma-test-support` depend broadly on product crates unless those dependencies are genuinely needed by test-only fixtures

### `crates/soma/web`

Own the Rust crate that embeds or serves Soma's compiled frontend assets and product-specific fallback router.

This is distinct from:

```text
apps/web
    editable frontend source/application

crates/soma/web
    Rust-side product integration and embedded assets

crates/shared/http-server
    generic server and middleware helpers
```

---

## 4. Dependency graph

## 4.1 Target graph

```text
apps/soma
    ├── soma-cli ─────────────▶ soma-cli-core
    ├── soma-api ─────────────▶ soma-http-api
    │                           soma-http-server
    ├── soma-palette ─────────▶ soma-http-api
    ├── soma-mcp ─────────────▶ soma-mcp-server ───▶ rmcp-traces
    ├── soma-runtime ─────────▶ soma-application ──▶ soma-domain
    └── soma-integrations
            ├── soma-auth
            ├── soma-observability
            ├── soma-client
            ├── soma-provider-adapters ───▶ soma-provider-core
            ├── soma-gateway ────────────▶ soma-mcp-proxy
            │                              ├── soma-mcp-client
            │                              └── soma-mcp-server
            ├── soma-codemode ───────────▶ soma-openapi
            └── soma-openapi

soma-api, soma-palette, soma-mcp, and soma-cli also call soma-application for product use cases.
soma-http-server is composed by apps/soma and/or soma-api where HTTP serving is needed.

apps/palette/src-tauri
    ├── soma-tauri-shell
    └── soma-palette
```

`apps/soma` also depends on configuration, shared observability, runtime, web, palette, and plugin support as required by features.

## 4.2 Mandatory dependency rules

### Shared layer

```text
crates/shared/*
crates/shared/mcp/*
    may depend on external crates
    may depend on crates/shared/* or crates/shared/mcp/*
    may not depend on crates/soma/*
    may not depend on apps/*
```

This rule applies to optional dependencies and all Cargo features.

### Domain

```text
soma-domain
    may depend on small external value/serialization crates when justified
    may depend on carefully selected shared contract crates
    may not depend on application, integrations, runtime, API, MCP, CLI, or apps
```

### Application

```text
soma-application
    may depend on soma-domain
    may depend on neutral shared contracts such as soma-provider-core
    defines ports for concrete engines
    may not depend on Axum, Clap, RMCP transport types, Reqwest,
    soma-gateway, soma-codemode, soma-openapi, soma-client, or apps/soma
```

An exception may exist temporarily during the strangler migration when `soma-application` wraps legacy `soma-service`. Track and remove that exception.

### Integrations

```text
soma-integrations
    may depend on soma-application ports
    may depend on concrete shared engines and product implementations
    may not be imported by soma-domain
```

### Surface adapters

```text
soma-api
soma-palette
soma-mcp
soma-cli
    depend on soma-application
    depend on their respective shared crate or MCP role crate
    may depend on soma-domain value types when needed
    may not construct or directly dispatch provider engines
    may not depend on one another
```

### Runtime

```text
soma-runtime
    depends on soma-application and product runtime support
    does not expose lower-level engine internals to surfaces
```

### App

```text
apps/soma
    is the server/CLI composition root and may depend broadly across product/shared crates
    owns composition, not business policy

apps/palette/src-tauri
    is the desktop composition package and may depend on soma-tauri-shell and soma-palette
    owns desktop app wiring, not product/server policy
```

## 4.3 Shared-layer DAG

Recommended direction:

```text
rmcp-traces                         leaf
soma-auth                           external + optional rmcp
soma-observability                  external
soma-openapi                        leaf
soma-http-api                       external + optional axum/openapi
codex-app-server-client             leaf

soma-codemode ────────────────▶ soma-openapi          optional
soma-mcp-server ──────────────▶ rmcp-traces           optional
soma-mcp-proxy ───────────────▶ soma-mcp-client
soma-mcp-proxy ───────────────▶ soma-mcp-server
soma-gateway ─────────────────▶ soma-mcp-client
soma-gateway ─────────────────▶ soma-mcp-server
soma-gateway ─────────────────▶ soma-mcp-proxy
soma-gateway ─────────────────▶ soma-codemode          optional
soma-gateway ─────────────────▶ soma-openapi           optional

soma-provider-adapters ───────▶ soma-provider-core
soma-provider-adapters ───────▶ soma-openapi           optional
soma-provider-adapters ───────▶ soma-codemode          optional
soma-provider-adapters ───────▶ soma-gateway           optional

soma-http-server                    independent
soma-cli-core                       independent
soma-tauri-shell                    external + tauri
```

Do not introduce cycles among shared crates. If gateway and provider adapters need each other in both directions, extract the shared contract or keep one direction through an adapter owned by the higher layer.

Treat this DAG as a maximum, not a shopping list. A shared crate should start with the smallest dependency cone that lets it be useful. Add an internal shared dependency only when the public API becomes clearer, a duplicated invariant disappears, or the consumer avoids more glue than the dependency costs.

---

## 5. Where the business for a tool goes

Suppose Soma exposes one operation through all three surfaces:

```text
CLI:   soma gateway reload
REST:  POST /v1/gateway/reload
MCP:   gateway_reload
```

The ownership is:

```text
crates/soma/cli
    Defines `soma gateway reload`, parses flags, asks for interactive confirmation,
    renders success/failure.

crates/soma/api
    Defines POST /v1/gateway/reload, parses HTTP auth/request data,
    maps errors to status codes.

crates/soma/mcp
    Defines gateway_reload tool schema and MCP result/error mapping.

crates/soma/application
    Implements GatewayReload use case.
    Checks product authorization and confirmation.
    Applies product audit and response policy.
    Calls GatewayControl port.

crates/soma/domain
    Owns any invariant rule, such as protected routes that may not be removed.

crates/soma/integrations
    Implements GatewayControl with SomaGatewayAdapter.
    Translates product types into gateway types.

crates/shared/mcp/gateway
    Performs generic gateway reload and route/catalog lifecycle.

apps/soma
    Constructs the gateway, adapter, application, runtime, and surfaces.
```

Flow:

```text
CLI command ─┐
REST route ──┼──▶ SomaApplication::gateway_reload(...)
MCP tool ────┘                     │
                                   ▼
                         GatewayControl port
                                   │
                                   ▼
                     SomaGatewayAdapter
                                   │
                                   ▼
                         soma-gateway
```

The same structure applies to OpenAPI and Code Mode:

```text
CLI/API/MCP
    │
    ▼
soma-application use case
    │
    ├──▶ OpenApiExecutor port ──▶ soma-integrations ──▶ soma-openapi
    └──▶ CodeModeExecutor port ─▶ soma-integrations ──▶ soma-codemode
```

### Domain versus application

Use this dividing line:

```text
soma-domain
    What must always be true?

soma-application
    What sequence of work completes the user's operation?
```

Most "business for a tool" belongs in `soma-application`. Only invariant rules and behavior intrinsic to product entities belong in `soma-domain`.

---

## 6. Current-to-target mapping

## 6.1 Physical crate moves

| Current or incoming path | Target path | Package remains |
|---|---|---|
| `crates/soma` | `apps/soma` | `soma` |
| `crates/rmcp-traces` | `crates/shared/traces` | `rmcp-traces` |
| `crates/soma-auth` | `crates/shared/auth` | `soma-auth` |
| `crates/soma-observability` | `crates/shared/observability` | `soma-observability` |
| new extraction from `crates/soma-api` | `crates/shared/http-api` | `soma-http-api` |
| `crates/soma-openapi` | `crates/shared/openapi` | `soma-openapi` |
| `crates/soma-codemode` | `crates/shared/codemode` | `soma-codemode` |
| new extraction from `apps/palette/src-tauri` | `crates/shared/tauri-shell` | `soma-tauri-shell` |
| `crates/soma-mcp-client` | `crates/shared/mcp/client` | `soma-mcp-client` |
| `crates/soma-mcp-server` | `crates/shared/mcp/server` | `soma-mcp-server` |
| `crates/soma-mcp-proxy` | `crates/shared/mcp/proxy` | `soma-mcp-proxy` |
| `crates/soma-gateway` | `crates/shared/mcp/gateway` | `soma-gateway` |
| `crates/codex-app-server-client` | `crates/shared/codex-app-server-client` | unchanged |
| `crates/soma-api` | `crates/soma/api` | `soma-api` |
| `crates/soma-cli` | `crates/soma/cli` | `soma-cli` |
| `crates/soma-mcp` | `crates/soma/mcp` | `soma-mcp` |
| new extraction from Palette routes/app contract | `crates/soma/palette` | `soma-palette` |
| `apps/palette/src-tauri` | remains app-local | `soma-palette-tauri` |
| `crates/soma-runtime` | `crates/soma/runtime` | unchanged |
| `crates/soma-test-support` | `crates/soma/test-support` | unchanged |
| `crates/soma-web` | `crates/soma/web` | unchanged |
| `crates/soma-service` | `crates/soma/service` temporarily | `soma-service` |
| `crates/soma-contracts` | `crates/soma/contracts` temporarily | `soma-contracts` |

The temporary `service` and `contracts` paths make their product ownership explicit while preserving package names during migration. They disappear after their responsibilities have moved.

## 6.2 Source responsibility moves

### From `soma-service`

```text
app.rs and shared workflows
    → soma-application

soma.rs or concrete remote HTTP client
    → soma-client

provider_registry.rs and provider_registry/*
    → soma-provider-core

capabilities.rs and generic provider errors
    → soma-provider-core

providers/* that are generic
    → soma-provider-adapters

providers/* that call Soma or rely on Soma runtime/auth
    → soma-integrations
```

### From `soma-contracts`

```text
config.rs and env_registry.rs
    → soma-config

providers.rs and provider_validation.rs
    → soma-provider-core

actions.rs
    split by ownership:
        generic provider `ToolSpec` metadata → soma-provider-core
        product use-case request/results → soma-application
        invariant product values → soma-domain
        API DTOs → soma-api
        MCP DTOs → soma-mcp
        CLI DTOs → soma-cli

token_limit.rs
    product response policy → soma-application
    generic byte/token helper, only if reusable → appropriate shared crate

errors.rs
    split by the layer that creates each error
```

### From current runtime and app package

```text
apps/soma/src/routes.rs
    → apps/soma/src/http.rs for composition
    → soma-api for product REST routes
    → soma-mcp for product HTTP MCP adapter
    → soma-http-server for generic listener/middleware

apps/soma/src/runtime.rs
    → apps/soma/src/bootstrap.rs for construction
    → apps/soma/src/local.rs for one-shot CLI mode
    → apps/soma/src/stdio.rs for stdio mode
    → soma-runtime for task supervision/readiness

soma-runtime AppState fields
    SomaService + ProviderRegistry
    → Arc<SomaApplication>
```

### From current Palette app

```text
apps/palette/src-tauri/src/lib.rs window/tray/shortcut/blur helpers
    → soma-tauri-shell

apps/palette/src-tauri/src/persistence.rs generic JSON app-data helpers
    → soma-tauri-shell where product-neutral
    → stays in apps/palette/src-tauri when tied to Palette settings shape

apps/palette/src-tauri/src/labby_bridge.rs
    → apps/palette/src-tauri for app-local HTTP client wiring
    → soma-palette for shared product DTOs, endpoint constants, and error shapes

apps/palette/src-tauri/src/oauth/*
    → apps/palette/src-tauri unless it becomes product-neutral enough for soma-auth

frontend React components and CSS
    → stay in apps/palette/src
```

### From current Palette HTTP/provider code

```text
provider registry cached_palette_manifest and Palette report projection
    → soma-palette for product manifest shape
    → soma-provider-core for generic ToolSpec Palette overlay metadata

/v1/palette/catalog
/v1/palette/search
/v1/palette/schema
/v1/palette/execute
    → soma-palette routes
```

---

## 7. Workspace manifest

Recommended root structure:

```toml
[workspace]
resolver = "2"
members = [
    "apps/soma",
    "crates/shared/auth",
    "crates/shared/cli-core",
    "crates/shared/codemode",
    "crates/shared/codex-app-server-client",
    "crates/shared/http-api",
    "crates/shared/http-server",
    "crates/shared/mcp/client",
    "crates/shared/mcp/gateway",
    "crates/shared/mcp/proxy",
    "crates/shared/mcp/server",
    "crates/shared/observability",
    "crates/shared/openapi",
    "crates/shared/provider-adapters",
    "crates/shared/provider-core",
    "crates/shared/tauri-shell",
    "crates/shared/traces",
    "crates/soma/api",
    "crates/soma/application",
    "crates/soma/cli",
    "crates/soma/client",
    "crates/soma/config",
    "crates/soma/domain",
    "crates/soma/integrations",
    "crates/soma/mcp",
    "crates/soma/palette",
    "crates/soma/runtime",
    "crates/soma/test-support",
    "crates/soma/web",
    "xtask",
]
```

Enumerate members explicitly rather than globbing. This matches the live manifest
and avoids two hazards: a `crates/shared/*` glob would try to treat the
`crates/shared/mcp/` grouping directory as a package, and a `crates/soma/*` glob
silently picks up any stray directory that appears under the product tree. The
cost of the explicit list is one line per crate at creation time; the benefit is
that workspace membership is reviewable in the diff.

Keep non-Rust frontend directories such as `apps/web` outside Cargo membership. Keep `apps/palette/src-tauri` as an app-local Tauri workspace/package by default; it may depend on root workspace crates by path but does not need to be a root workspace member.

Centralize all internal paths.

> **Outstanding as of 2026-07-16.** The root manifest has no
> `[workspace.dependencies]` table at all. Every crate declares raw relative
> paths (`soma-runtime = { path = "../../crates/soma/runtime" }`) and the rmcp
> pin is copy-pasted across eight manifests. Section 9.1 assigned this to PR 0
> and PR 0 is marked completed, because the *outcome* it tested for — a single
> resolved rmcp version — holds. The mechanism was never built, and no remaining
> slice picks it up. PR 10 owns it now; see its prerequisite step.

```toml
[workspace.dependencies]
# Shared
rmcp-traces = { path = "crates/shared/traces" }
soma-auth = { path = "crates/shared/auth" }
soma-observability = { path = "crates/shared/observability" }
soma-openapi = { path = "crates/shared/openapi" }
soma-codemode = { path = "crates/shared/codemode" }
soma-http-api = { path = "crates/shared/http-api" }
soma-mcp-client = { path = "crates/shared/mcp/client" }
soma-mcp-server = { path = "crates/shared/mcp/server" }
soma-mcp-proxy = { path = "crates/shared/mcp/proxy" }
soma-gateway = { path = "crates/shared/mcp/gateway" }
soma-provider-core = { path = "crates/shared/provider-core" }
soma-provider-adapters = { path = "crates/shared/provider-adapters" }
soma-http-server = { path = "crates/shared/http-server" }
soma-cli-core = { path = "crates/shared/cli-core" }
soma-tauri-shell = { path = "crates/shared/tauri-shell" }
codex-app-server-client = { path = "crates/shared/codex-app-server-client" }

# Soma product
soma-domain = { path = "crates/soma/domain" }
soma-application = { path = "crates/soma/application" }
soma-config = { path = "crates/soma/config" }
soma-client = { path = "crates/soma/client" }
soma-integrations = { path = "crates/soma/integrations" }
soma-runtime = { path = "crates/soma/runtime" }
soma-api = { path = "crates/soma/api" }
soma-mcp = { path = "crates/soma/mcp" }
soma-cli = { path = "crates/soma/cli" }
soma-palette = { path = "crates/soma/palette" }
soma-test-support = { path = "crates/soma/test-support" }
soma-web = { path = "crates/soma/web" }
```

Align protocol/framework versions centrally, especially RMCP:

```toml
[workspace.dependencies]
rmcp = "=2.2.0"
```

Use one exact RMCP version for crates that exchange RMCP types in-process. A deliberate process or serialization boundary may permit another version, but document it.

### Architecture metadata

Add machine-readable metadata to each internal package:

```toml
[package.metadata.soma-architecture]
layer = "shared"
```

Allowed values:

```text
app
shared
product-domain
product-application
product-integration
product-runtime
product-surface
product-support
legacy
```

`cargo xtask check-architecture` can validate both path and metadata.

---

## 8. Feature ownership

The app package is the product feature aggregator.

Suggested shape:

```toml
# apps/soma/Cargo.toml
[features]
default = ["full"]

cli = ["dep:soma-cli", "dep:soma-cli-core"]
mcp = ["dep:soma-mcp"]
mcp-stdio = ["mcp"]
mcp-http = ["mcp", "api"]
api = ["dep:soma-api", "dep:soma-http-api", "dep:soma-http-server"]
palette = ["api", "dep:soma-palette"]
auth = ["dep:soma-auth"]
oauth = ["auth"]
web = ["api", "dep:soma-web"]
observability = ["dep:soma-observability"]
plugin = ["cli"]
gateway = ["dep:soma-gateway", "soma-integrations/gateway"]
codemode = ["dep:soma-codemode", "soma-integrations/codemode"]
openapi = ["dep:soma-openapi", "soma-integrations/openapi"]

local-adapter = ["cli", "mcp-stdio"]
server = ["cli", "api", "mcp-http", "mcp-stdio"]
full = [
    "server",
    "auth",
    "oauth",
    "palette",
    "web",
    "observability",
    "plugin",
    "gateway",
    "codemode",
    "openapi",
]
```

`apps/palette/src-tauri/Cargo.toml` can stay app-local and still depend on shared/product crates by path:

```toml
soma-tauri-shell = { path = "../../../crates/shared/tauri-shell" }
soma-palette = { path = "../../../crates/soma/palette" }
```

Exact features should follow current product behavior. The architectural rules are:

1. Shared crate features activate only external or shared dependencies.
2. A feature in `soma-gateway` named `oauth` must activate generic gateway OAuth support, not `soma-auth`.
3. Product integration features live in `soma-integrations` and are selected by `apps/soma`.
4. Surface crates do not secretly construct engines through features.
5. CI tests no-default, default, and all-features graphs.

---

## 9. Pre-merge gates for the four incoming crates

Do these before declaring the new shared layer stable.

## 9.1 Align RMCP

The incoming trace and gateway branches must use the same RMCP version when they exchange RMCP types in-process.

Actions:

1. Set RMCP in `[workspace.dependencies]`.
2. Change all participating crates to `rmcp.workspace = true`.
3. Run:

```bash
cargo tree -d | rg 'rmcp|modelcontext'
cargo tree -p rmcp-traces --all-features
cargo tree -p soma-gateway --all-features
```

Acceptance:

- one intended RMCP version in the relevant graph
- no duplicate protocol type universe

## 9.2 Split gateway MCP mechanics into role crates

The reviewed gateway branch already contains outbound client, inbound server-adjacent proxying, and gateway composition code in one package. Split it before declaring the gateway reusable.

Actions:

1. Move outbound upstream session/pool/discovery/call/transport/process/security modules into `soma-mcp-client`.
2. Move reusable inbound lifecycle, response paging, protocol result shaping, and trace helpers into `soma-mcp-server`.
3. Move route naming, catalog projection, subject propagation, protected subset filtering, and inbound-to-upstream dispatch into `soma-mcp-proxy`.
4. Leave `soma-gateway` as the composed manager/config/runtime layer that wires the role crates together.
5. Add standalone construction tests for each role crate.

Acceptance:

```bash
cargo test -p soma-mcp-client --all-features
cargo test -p soma-mcp-server --all-features
cargo test -p soma-mcp-proxy --all-features
cargo tree -p soma-mcp-client --all-features
cargo tree -p soma-mcp-server --all-features
cargo tree -p soma-mcp-proxy --all-features
```

No role crate depends on `soma-gateway`, `crates/soma/*`, or `apps/soma`.

## 9.3 Remove gateway's `soma-auth` dependency and product policy

Actions:

1. Define generic auth/OAuth traits or credential resolvers in `soma-gateway` and `soma-mcp-client`.
2. Remove the optional path dependency on `soma-auth`.
3. Move the current translation into `soma-integrations::gateway_auth` after the product integration crate exists.
4. Until then, place a temporary bridge in the app package only if needed to keep the PR buildable.
5. Add a dependency-boundary test that runs with `--all-features`.
6. Replace `SOMA_HOME`, `.soma`, `soma_home`, `soma:read`, `soma:write`, `soma.gateway.error.v1`, and `soma://upstream/` defaults with explicit gateway configuration or product-supplied policies.
7. Remove gateway special-casing for a native tool named `soma`; make reserved tool names a host-supplied routing policy.

Acceptance:

```bash
cargo tree -p soma-gateway --all-features
```

contains no `soma-*` product crate from `crates/soma`.

## 9.4 Remove duplicated Code Mode and OpenAPI engines from gateway

Actions:

1. Inventory `soma-gateway` code under Code Mode and OpenAPI modules.
2. Classify each item as:
   - gateway routing/projection
   - generic engine behavior
3. Keep routing/projection in gateway.
4. Delegate generic behavior to `soma-codemode` or `soma-openapi` through optional dependencies.
5. Add integration tests proving the feature bridges work.

Acceptance:

- one canonical Code Mode runner implementation
- one canonical OpenAPI executor
- gateway modules are adapters, not forks

## 9.5 Standalone construction tests

Each incoming crate must have a test or example that constructs it without Soma product configuration:

```text
rmcp-traces
    parse explicit metadata

soma-openapi
    construct registry/client from explicit config

soma-codemode
    construct runner/pool from explicit config

soma-mcp-client
    construct upstream pool from explicit config and fake/in-process upstream

soma-mcp-server
    construct paging/protocol helpers without product state

soma-mcp-proxy
    construct proxy routes from fake upstream snapshots

soma-gateway
    construct gateway from explicit config and fake auth hooks
```

---

## 10. Implementation plan

Use small, behavior-preserving PRs. Do not combine the physical move, application boundary, engine extraction, and surface rewrite in one crate-shaped thunderstorm.

## PR 0: Merge preparation and shared-crate corrections

### Goal

Land the incoming traces, OpenAPI, Code Mode, and gateway work with valid standalone boundaries, then split gateway-owned MCP mechanics into reusable role crates.

### Scope

- merge/rebase traces, OpenAPI, Code Mode, and gateway work
- align RMCP versions
- extract `soma-mcp-client`, `soma-mcp-server`, and `soma-mcp-proxy` from gateway/MCP overlap
- remove product-shaped gateway auth defaults and ensure any auth dependency resolves to shared `soma-auth`
- remove Soma-shaped gateway defaults from the shared layer
- reconcile gateway Code Mode/OpenAPI modules with the standalone engines
- add direct package tests
- run all-features dependency audits

### Acceptance

```bash
cargo test -p rmcp-traces --all-features
cargo test -p soma-openapi --all-features
cargo test -p soma-codemode --all-features
cargo test -p soma-mcp-client --all-features
cargo test -p soma-mcp-server --all-features
cargo test -p soma-mcp-proxy --all-features
cargo test -p soma-gateway --all-features
cargo tree -p soma-mcp-client --all-features
cargo tree -p soma-mcp-server --all-features
cargo tree -p soma-mcp-proxy --all-features
cargo tree -p soma-gateway --all-features
```

No incoming crate depends on a current Soma product crate.

---

## PR 1: Freeze post-merge behavior

### Goal

Create a safety net before moving architecture.

### Add or update snapshots for

- CLI help and command discovery
- human CLI output
- JSON CLI output
- REST routes and payloads
- generated OpenAPI
- MCP tool list and schemas
- MCP prompts and resources
- provider catalog and fingerprint
- dynamic provider CLI/REST/MCP parity
- auth scopes
- destructive-action confirmation
- response caps and paging
- local mode
- remote mode
- stdio MCP
- Streamable HTTP MCP
- gateway catalog/routes/admin behavior
- Code Mode execution
- OpenAPI execution

### Run the existing gates

```bash
cargo xtask contract-audit
cargo xtask generate-provider-surfaces --check
cargo xtask check-schema-docs --check
cargo xtask check-openapi --check
cargo xtask validate-plugin-layout
cargo xtask check-version-sync
just verify
```

Build current profiles:

```bash
cargo build --bin soma --no-default-features --features local-adapter
cargo build --bin soma --no-default-features --features server
cargo build --bin soma --features full
```

### Acceptance

A later refactor PR can prove that behavior did not change.

---

## PR 2: Apply the physical workspace taxonomy

### Goal

Adopt the chosen `apps`, `crates/shared`, and `crates/soma` paths with no intentional Rust behavior changes.

### Moves

```bash
mkdir -p crates/shared
mkdir -p crates/shared/mcp

git mv crates/soma apps/soma
mkdir -p crates/soma

# Incoming standalone crates
git mv crates/rmcp-traces crates/shared/traces
git mv crates/soma-auth crates/shared/auth
git mv crates/soma-observability crates/shared/observability
git mv crates/soma-openapi crates/shared/openapi
git mv crates/soma-codemode crates/shared/codemode
git mv crates/soma-mcp-client crates/shared/mcp/client
git mv crates/soma-mcp-server crates/shared/mcp/server
git mv crates/soma-mcp-proxy crates/shared/mcp/proxy
git mv crates/soma-gateway crates/shared/mcp/gateway

# Existing standalone client
git mv crates/codex-app-server-client crates/shared/codex-app-server-client

# Existing Soma product crates
git mv crates/soma-api crates/soma/api
git mv crates/soma-cli crates/soma/cli
git mv crates/soma-contracts crates/soma/contracts
git mv crates/soma-mcp crates/soma/mcp
git mv crates/soma-runtime crates/soma/runtime
git mv crates/soma-service crates/soma/service
git mv crates/soma-test-support crates/soma/test-support
git mv crates/soma-web crates/soma/web
```

Package names remain unchanged in this PR.

### Update

- root workspace members
- `[workspace.dependencies]` paths
- every path dependency
- xtask path constants
- scaffolding templates
- cargo-generate templates
- release scripts
- Docker builds
- CI paths and filters
- docs and architecture diagrams
- `include_str!`, asset, schema, fixture, and migration paths
- package publishing/include lists
- npm launcher references if path-sensitive

### Guardrails

- no code cleanup in this PR
- no type moves
- no package renames
- no feature redesign
- preserve Git rename detection

### Acceptance

All baseline gates from PR 1 pass unchanged.

### Why move early now

The physical taxonomy is no longer undecided. A single path-only PR prevents every later architecture PR from repeatedly changing the same manifests and documentation.

---

## PR 3: Add architecture enforcement

### Goal

Make the folder boundaries executable policy.

### Add

```bash
cargo xtask check-architecture
```

Use `cargo metadata --all-features` to classify each internal package by path and package metadata.

### Enforce

1. Shared packages under `crates/shared/*` and `crates/shared/mcp/*` have no dependency on `crates/soma/*` or `apps/*`.
2. The rule includes optional dependencies.
3. `soma-domain` does not depend on surfaces, runtime, application, integrations, or app.
4. `soma-application` does not depend on surfaces, runtime, app, or concrete engines.
5. `soma-api`, `soma-mcp`, and `soma-cli` do not depend on one another.
6. Only integration/composition layers may depend on both product application ports and concrete shared engines.
7. No cycles exist among internal crates.
8. Shared crate all-features graphs remain shared-only.

### Temporary exceptions

Record explicit, expiring exceptions for the strangler migration:

```text
soma-application → soma-service
soma-application → soma-contracts
```

Every exception includes:

- owner
- reason
- removal PR
- expiration milestone

### CI

Run the check before expensive tests so boundary failures are fast.

---

## PR 4: Introduce `soma-domain` and `soma-application`

### Goal

Create one shared product boundary before moving implementation internals.

### Create

```text
crates/soma/domain
crates/soma/application
```

### Initial implementation

Use a strangler facade:

```rust
pub struct SomaApplication {
    legacy_service: Arc<SomaService>,
    legacy_registry: Arc<ProviderRegistry>,
    // gateway and other existing handles as temporary dependencies
}
```

Expose product-oriented methods:

```rust
execute_action(...)
catalog_snapshot(...)
refresh_providers(...)
read_resource(...)
list_resources(...)
list_prompts(...)
get_prompt(...)
gateway_status(...)
gateway_reload(...)
gateway_execute(...)
codemode_execute(...)
openapi_execute(...)
status(...)
readiness(...)
doctor(...)
```

Define:

- `ExecutionContext`
- application requests and responses
- `ApplicationError`
- port traits for concrete engines
- small domain values and rules that are already clear

### Important

Do not move all legacy code yet. The purpose of this PR is to establish the stable inward-facing API that surfaces will call.

### Tests

For each use case, test:

- authorization
- confirmation
- defaults
- response limiting
- error normalization
- trace/request context propagation

### Acceptance

The application facade can execute one representative action, one gateway operation, one Code Mode operation, and one OpenAPI operation using existing internals.

---

## PR 5: Migrate the CLI to `SomaApplication`

### Goal

Make the CLI a thin product adapter.

### Remove from `soma-cli`

- `SomaClient::new`
- `SomaService::new`
- provider registry construction
- provider refresh orchestration
- direct `ProviderCall` construction where an application request replaces it
- direct registry dispatch
- independent local/remote business policy

### Keep in `soma-cli`

- command parsing
- dynamic command input collection
- interactive confirmation I/O
- terminal and JSON rendering
- exit-code mapping

### Target API

```rust
pub async fn run(
    app: Arc<SomaApplication>,
    invocation: CliInvocation,
    io: &mut dyn CliIo,
) -> Result<ExitCode, CliError>;
```

### Acceptance query

```bash
rg 'SomaClient::new|SomaService::new|dynamic_provider_registry|ProviderCall|\.dispatch\(' \
  crates/soma/cli
```

Expected result: no direct lower-level construction or dispatch.

---

## PR 6: Migrate the REST API to `SomaApplication`

### Goal

Make HTTP handlers thin translation layers.

### Change

- `ApiState` holds `Arc<SomaApplication>`
- route lookup calls application catalog/query methods
- handlers convert HTTP input to application requests
- application errors map to HTTP responses in `soma-api`
- OpenAPI generation continues to describe product routes without owning execution logic

### Remove

- direct provider registry refresh
- direct provider snapshot access where product application query suffices
- direct dispatch
- business response-cap logic

### Keep

- paths and methods
- extractors
- HTTP DTOs
- headers
- status codes
- API-specific pagination representation

### Acceptance query

```bash
rg 'SomaService|ProviderRegistry|ProviderCall|\.dispatch\(' crates/soma/api
```

Expected result: no direct business engine dispatch.

---

## PR 7: Migrate MCP to `SomaApplication`

### Goal

Make MCP tools, prompts, and resources transport adapters.

### Change

- `McpState` holds `Arc<SomaApplication>`
- tools call application use cases
- prompts/resources call application queries
- trace metadata becomes `ExecutionContext.trace`
- application errors map to MCP errors in `soma-mcp`

### Keep

- tool names and descriptions
- input/output JSON schemas
- protocol result construction
- MCP-specific pagination tokens until `soma-mcp-server` extraction

### Remove

- direct registry dispatch
- product authorization workflow
- product response-limit workflow
- direct gateway engine control

### Acceptance query

```bash
rg 'SomaService|ProviderRegistry|ProviderCall|\.dispatch\(' crates/soma/mcp
```

Expected result: no direct lower-level dispatch.

---

## PR 8: Convert runtime state to the application facade

### Goal

Stop exposing the legacy service and registry to every surface.

### Change

```rust
pub struct SomaRuntime {
    application: Arc<SomaApplication>,
    supervisor: TaskSupervisor,
    readiness: ReadinessHandle,
}
```

Move provider refresh loops and long-running tasks behind application/runtime interfaces.

### Remove from public runtime state

- raw `SomaService`
- raw `ProviderRegistry`
- raw shared engine handles unless needed for process administration

### Acceptance

API, MCP, and CLI receive only the application/runtime interfaces they need.

---

## PR 9: Extract `soma-provider-core`

### Goal

Move the generic provider model and registry out of legacy product service/contracts.

### Move

- provider manifests
- provider validation
- capabilities
- `ToolSpec`, prompt, resource, task, and elicitation metadata
- provider trait
- registry builder
- indexes
- snapshots
- fingerprints
- generic dispatch
- generic provider errors

### Strip

- Soma product scope assumptions
- Soma config paths
- Soma auth types
- transport DTOs
- product process lifecycle

### Tests

Create a standalone fake provider that registers and dispatches without constructing any Soma product type.

### Acceptance

```bash
cargo test -p soma-provider-core --all-features
cargo tree -p soma-provider-core --all-features
```

The dependency graph is shared-only.

---

## PR 10: Extract reusable provider adapters and reconcile engines

### Goal

Create one adapter layer instead of parallel OpenAPI, Code Mode, gateway, and provider implementations.

### Prerequisite: centralize internal paths

Build the `[workspace.dependencies]` table from section 7 before moving any code.
It does not exist today; section 9.1 assigned it to PR 0 and it was not done.

Do it here because every remaining slice relocates crates, and each relocation
without the table means hand-editing every consumer's relative path instead of
one workspace entry. PRs 12, 13, and 19 are the worst affected. The rmcp pin,
currently duplicated across eight manifests, moves to `rmcp.workspace = true` at
the same time.

```bash
cargo tree -d | rg 'rmcp|modelcontext'   # still one rmcp version afterward
```

This is mechanical and behavior-preserving. Land it as its own commit inside
PR 10, or as a standalone PR ahead of it — do not interleave it with the adapter
extraction.

### Move generic providers

- manifest/file-backed provider
- static Rust provider abstraction
- TypeScript/Python sidecar providers where product-neutral
- WASM provider
- AI SDK provider

### Add bridges

```text
provider-adapters::openapi
    delegates to soma-openapi

provider-adapters::codemode
    delegates to soma-codemode

provider-adapters::gateway
    projects gateway catalog/routes through soma-provider-core
```

### Upstream MCP decision

Treat `soma-gateway` as the canonical upstream MCP connection/routing engine. Migrate or remove the older upstream MCP provider implementation when capabilities overlap. Preserve only a thin provider projection adapter.

### Product-specific providers

Move remote-Soma or auth/runtime-aware providers to:

```text
crates/soma/integrations
```

### Acceptance

- no duplicate OpenAPI HTTP executor
- no duplicate Code Mode runner
- no second upstream MCP transport stack
- shared adapters have no product dependencies

---

## PR 11: Create `soma-integrations`

### Goal

Centralize product adapters to shared engines and infrastructure.

### Implement application ports

- `ProviderRuntime`
- `GatewayControl`
- `CodeModeExecutor`
- `OpenApiExecutor`
- `UpstreamGateway`
- `Authorizer`
- `AuditSink`

### Add the gateway/auth bridge

```rust
pub struct SomaGatewayAuthenticator {
    auth: Arc<SomaAuth>,
}

impl soma_gateway::GatewayAuthenticator for SomaGatewayAuthenticator {
    // Translate generic gateway credentials and principals.
}
```

### Move from app package

Any temporary gateway/auth bridge added for mergeability now moves from `apps/soma` into `soma-integrations`.

### Acceptance

`apps/soma` constructs adapters but does not contain their implementation logic.

---

## PR 12: Split `soma-service`

### Goal

Remove the legacy multi-layer crate.

### Create

```text
crates/soma/client
```

It does not exist yet, and the "remote Soma HTTP client" move below assumes it as
a destination. Create it as an empty package with
`[package.metadata.soma-architecture] layer = "product-support"`, register it in
the root workspace members list, then move code into it. (`soma-integrations`,
the other destination below, is created by PR 11.)

### Move

```text
business workflows
    → soma-application

invariant rules
    → soma-domain

remote Soma HTTP client
    → soma-client

generic provider framework
    → soma-provider-core

generic concrete providers
    → soma-provider-adapters

product engine bridges
    → soma-integrations
```

### Compatibility stage

For one migration window, `soma-service` may re-export new types:

```rust
#[deprecated(note = "use soma_application")]
pub use soma_application::*;
```

No new code may be added to the legacy crate.

### Acceptance

All non-test consumers use the new crates. Delete `soma-service` when downstream templates and generators are updated.

---

## PR 13: Split `soma-contracts`

### Goal

Replace the shared-type drawer with owner-specific contracts.

### Create

```text
crates/soma/config
```

It does not exist yet, and the configuration/environment move below assumes it as
a destination. Create it as an empty package with
`[package.metadata.soma-architecture] layer = "product-support"` and register it
in the root workspace members list before moving code into it.

### Move by ownership

```text
configuration/environment
    → soma-config

provider manifests and validation
    → soma-provider-core

application requests/results
    → soma-application

invariant values
    → soma-domain

HTTP DTOs
    → soma-api

MCP DTOs
    → soma-mcp

CLI DTOs
    → soma-cli

errors
    → crate that creates the error
```

### Compatibility stage

Use deprecation re-exports for one migration window. Do not let the facade gain new types.

### Acceptance

No production crate depends on `soma-contracts`. Remove it after scaffolding and generated projects are updated.

---

## PR 14: Finish MCP role-crate cleanup

### Goal

Remove any remaining generic MCP mechanics from the product adapter after the initial PR 0 split.

### Candidates

- response page store leftovers into `soma-mcp-server`
- stdio lifecycle leftovers into `soma-mcp-server`
- HTTP MCP lifecycle leftovers into `soma-mcp-server`
- cancellation/shutdown hooks into `soma-mcp-server`
- protocol conversion helpers into `soma-mcp-server`
- upstream route projection leftovers into `soma-mcp-proxy`
- outbound session/pool leftovers into `soma-mcp-client`
- trace metadata extraction integration into `soma-mcp-server`
- conformance fixtures

### Keep in `soma-mcp`

- Soma tool schemas
- Soma prompts/resources
- product scope mapping
- application request translation
- product errors

### Acceptance

A fake unrelated MCP server can use `soma-mcp-server` without importing a Soma product crate. A fake unrelated gateway can use `soma-mcp-client` plus `soma-mcp-proxy` without importing Soma product crates.

---

## PR 15: Extract `soma-http-server`

### Goal

Move generic Axum server mechanics out of app and API crates.

### Candidates

- listener binding
- graceful shutdown
- request ID
- tracing middleware
- timeout and body limit
- generic CORS configuration
- generic health primitives
- rejection envelope

### Keep in product crates

```text
soma-api
    Soma routes and DTOs

soma-web
    Soma embedded UI

apps/soma/http.rs
    router composition and listener selection
```

### Acceptance

A fake unrelated Axum router can be served through `soma-http-server`.

---

## PR 16: Extract `soma-cli-core`

### Goal

Move reusable terminal mechanics out of the product CLI.

### Candidates

- output formats
- table and JSON rendering
- confirmation I/O
- terminal/color policy
- shell completions
- progress output

### Keep in `soma-cli`

- Soma parser and commands
- dynamic provider command projection
- mapping to application requests
- product exit-code policy

### Important

Do not combine a CLI parser rewrite with this extraction. Preserve current help and output snapshots. A future Clap migration can be its own behavior-controlled PR.

---

## PR 17: Extract `soma-palette` and `soma-tauri-shell`

### Goal

Separate reusable Tauri shell mechanics from Soma-specific Palette product behavior while keeping `apps/palette` as the desktop application package.

### Move to `soma-tauri-shell`

- product-neutral window show/hide/focus/resize helpers
- tray setup helpers
- shortcut parsing and rebind helpers
- blur-dismiss state/event helpers
- product-neutral app-data JSON persistence helpers
- generic Tauri command error/result helpers

### Move to `soma-palette`

- `/v1/palette/catalog`
- `/v1/palette/search`
- `/v1/palette/schema`
- `/v1/palette/execute`
- Palette DTOs shared by server and desktop app
- product mapping from provider `ToolSpec` Palette overlays to UI actions
- product launcher execution and auth policy
- product Palette route OpenAPI metadata

### Keep in `apps/palette`

- React/Vite frontend source
- Tauri package, `tauri.conf.json`, icons, capabilities, and bundle metadata
- command registration and app-local wiring
- server URL settings and app-specific HTTP bridge
- OAuth desktop flow until it is proven reusable enough for `soma-auth`

### Acceptance

```bash
cargo test -p soma-tauri-shell --all-features
cargo test -p soma-palette --all-features
pnpm --dir apps/palette test
```

The desktop app still builds from `apps/palette`, and the root Cargo workspace does not need to own the app-local Tauri package.

---

## PR 18: Slim `apps/soma` and finalize composition

### Goal

Make the binary package an unmistakable composition root.

### Finalize

- `bootstrap.rs` builds concrete graph
- `http.rs` composes product routers including `soma-palette` and calls `http-server`
- `stdio.rs` starts product MCP through `soma-mcp-server`
- `local.rs` invokes `soma-cli`
- `shutdown.rs` owns process signals
- `soma-runtime` owns background tasks/readiness
- `apps/soma` contains no business rules

### Optional binaries

Add `soma-api` or `soma-mcp` only after deciding they are real release artifacts. Otherwise keep one canonical `soma` binary and subcommands.

### Acceptance

The app package's modules are mostly constructors, mode selection, router merging, and lifecycle code.

---

## PR 19: Delete legacy facades and update ecosystem artifacts

### Goal

Finish the migration and remove temporary architecture debt.

### Delete

- `crates/soma/service`
- `crates/soma/contracts`
- temporary boundary exceptions
- old path aliases
- duplicated engine implementations

### Update

- README architecture section
- `docs/ARCHITECTURE.md`
- scaffold output layout
- cargo-generate template
- generated provider docs
- OpenAPI snapshots
- plugin metadata
- Dockerfiles
- CI workflows
- release-please configuration
- package include lists
- npm launcher documentation
- developer agent instructions

### Acceptance

`cargo xtask check-architecture` runs with zero exceptions.

---

## 11. Architecture enforcement details

## 11.1 `cargo xtask check-architecture`

Use Cargo metadata rather than grepping manifests only.

Pseudo-algorithm:

```text
load cargo metadata with all features
for each workspace package:
    classify by canonical path and package metadata
    inspect normal, build, dev, and optional dependencies
    validate allowed layer edges
    report the shortest violating edge
validate no internal dependency cycle
validate metadata matches physical path
```

Example failure:

```text
architecture violation:
  shared package soma-gateway
  depends on product package soma-config
  edge: soma-gateway --feature oauth -> soma-config

move the adapter to crates/soma/integrations or make the gateway hook generic
```

## 11.2 Source-level checks

Add focused checks for the surface boundary:

```bash
rg 'SomaClient::new|SomaService::new|dynamic_provider_registry|ProviderCall|ProviderRegistry' \
  crates/soma/cli crates/soma/api crates/soma/mcp
```

After migration, allow only DTO/type references explicitly approved by the architecture. Construction and dispatch should be absent.

## 11.3 Dependency checks

```bash
cargo tree -p rmcp-traces --all-features
cargo tree -p soma-auth --all-features
cargo tree -p soma-observability --all-features
cargo tree -p soma-openapi --all-features
cargo tree -p soma-codemode --all-features
cargo tree -p soma-http-api --all-features
cargo tree -p soma-gateway --all-features
cargo tree -p soma-provider-core --all-features
cargo tree -p soma-provider-adapters --all-features
cargo tree -p soma-mcp-client --all-features
cargo tree -p soma-mcp-server --all-features
cargo tree -p soma-mcp-proxy --all-features
cargo tree -p soma-http-server --all-features
cargo tree -p soma-cli-core --all-features
cargo tree -p soma-tauri-shell --all-features
cargo tree -p soma-palette --all-features
```

Fail CI when any tree reaches `crates/soma` or `apps/soma`.

Also check the default-feature trees for shared crates. Default features should not pull in unrelated surfaces such as gateway, web serving, OAuth providers, OpenAPI generation, storage, or product integration unless the crate's core purpose requires them.

---

## 12. Test strategy

## 12.1 Shared-crate tests

Every shared crate gets:

- direct unit tests
- at least one explicit-construction integration test
- no-default-features build when supported
- default-features dependency snapshot
- all-features build
- dependency-boundary check
- crates.io package include/list check before publish
- documentation examples
- no reliance on `SOMA_HOME` or product config in core tests

## 12.2 Application contract tests

Test each use case against fake ports:

- input validation
- authorization
- scope checks
- destructive confirmation
- engine call ordering
- retry and error behavior
- response limit behavior
- audit emission
- trace/request context propagation

Application tests should not start Axum, RMCP transports, or real subprocesses unless explicitly integration tests.

## 12.3 Adapter parity tests

For one operation, assert equivalent semantics across:

```text
CLI invocation
REST request
MCP call
```

The wire representation may differ. The application request, authorization decision, and business output must match.

## 12.4 Process/profile tests

Maintain:

```bash
cargo build --bin soma --no-default-features --features local-adapter
cargo build --bin soma --no-default-features --features server
cargo build --bin soma --features full
```

Smoke-test:

- `soma status`
- `soma doctor`
- `soma mcp`
- `soma serve`
- `/health`
- REST provider route
- HTTP MCP call
- stdio MCP call
- gateway route
- Code Mode call
- OpenAPI call

## 12.5 Full verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo xtask check-architecture
cargo xtask contract-audit
cargo xtask generate-provider-surfaces --check
cargo xtask check-schema-docs --check
cargo xtask check-openapi --check
cargo xtask validate-plugin-layout
cargo xtask check-version-sync
just verify
```

Adjust commands to the repository's exact xtask interface after the four PRs merge.

---

## 13. Definition of done

The refactor is complete when all statements are true.

### Physical organization

- server/CLI executable Rust product package is under `apps/soma`
- desktop Palette Tauri package remains under `apps/palette/src-tauri`
- every cross-project crate is under `crates/shared`
- every Soma product-only library is under `crates/soma`
- package names remain stable unless a separate naming decision changes them

### Shared boundaries

- no shared crate depends on a product crate under any feature
- gateway has no dependency on product auth/config/defaults; optional shared `soma-auth` use remains generic
- Code Mode and OpenAPI have one canonical implementation each
- shared crates construct from explicit configuration
- shared crates keep minimal default features and feature-gate heavyweight integrations
- each publishable shared crate has docs/examples that show blank-project usage

### Business boundary

- CLI, API, and MCP all call `SomaApplication`
- Palette routes call the same application/provider operation path instead of directly owning execution logic
- shared business operations live in `soma-application`
- invariant product rules live in `soma-domain`
- concrete engine bridges live in `soma-integrations`
- surfaces contain no direct provider/gateway/OpenAPI/Code Mode dispatch

### Runtime/composition

- runtime exposes application and supervised runtime handles, not a sack of internals
- app packages own construction, mode selection, native/app wiring, listeners, and shutdown
- app packages do not own business policy

### Legacy removal

- `soma-service` is removed
- `soma-contracts` is removed
- architecture checker has no temporary exceptions

### Behavior

- CLI, REST, MCP, Palette, OpenAPI, provider docs, plugins, auth, and feature profiles preserve their documented contracts unless a separate change explicitly versions them

---

## 14. Recommended immediate execution order

The first four moves should be:

1. **Fix and merge the standalone foundations.** Align RMCP, remove product-shaped gateway auth/config defaults, and eliminate duplicated OpenAPI/Code Mode engines.
2. **Freeze behavior.** Capture every current surface contract and feature build.
3. **Apply the physical taxonomy in one mechanical PR.** Move the app, shared crates, and product crates without changing their package names or internals.
4. **Introduce `SomaApplication`, then migrate CLI, API, and MCP one surface at a time.** Only after all surfaces use the facade should the legacy service/contracts internals be carved into their final crates.

This order gives Soma the selected map first, then builds the roads without rerouting traffic through the cornfield.

---

## 15. Compact ownership reference

| Concern | Owner |
|---|---|
| executable process and dependency construction | `apps/soma` |
| shared business use case behind CLI/API/MCP | `crates/soma/application` |
| invariant product rule | `crates/soma/domain` |
| product config and `SOMA_*` env | `crates/soma/config` |
| outbound remote Soma HTTP client | `crates/soma/client` |
| bridges from application ports to engines | `crates/soma/integrations` |
| initialized tasks/readiness | `crates/soma/runtime` |
| Soma REST routes | `crates/soma/api` |
| Soma MCP tools/prompts/resources | `crates/soma/mcp` |
| Soma CLI commands | `crates/soma/cli` |
| Soma Palette product routes/DTOs | `crates/soma/palette` |
| desktop Palette frontend/Tauri app | `apps/palette` |
| generic trace metadata | `crates/shared/traces` |
| reusable auth implementation | `crates/shared/auth` |
| reusable observability helpers | `crates/shared/observability` |
| reusable API response/error/probe helpers | `crates/shared/http-api` |
| generic OpenAPI engine | `crates/shared/openapi` |
| generic Code Mode runtime | `crates/shared/codemode` |
| generic outbound MCP client | `crates/shared/mcp/client` |
| generic inbound MCP server helpers | `crates/shared/mcp/server` |
| generic MCP inbound-to-upstream proxy | `crates/shared/mcp/proxy` |
| reusable MCP gateway engine | `crates/shared/mcp/gateway` |
| generic provider registry/contracts | `crates/shared/provider-core` |
| generic concrete provider implementations | `crates/shared/provider-adapters` |
| generic Axum lifecycle/middleware | `crates/shared/http-server` |
| generic terminal/CLI helpers | `crates/shared/cli-core` |
| generic Tauri shell helpers | `crates/shared/tauri-shell` |
| typed Codex app-server client | `crates/shared/codex-app-server-client` |
| gateway plus Soma auth adapter | `crates/soma/integrations/gateway_auth.rs` |
