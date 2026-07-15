# Soma Architecture Refactor Plan, Revision 3

**Repository:** `jmagar/soma`
**Date:** 2026-07-15
**Status:** Proposed target architecture and implementation sequence
**Supersedes:** `soma-architecture-refactor-plan-v2.md`

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
- `crates/shared/*` may be reused by another product without importing Soma product code.
- `crates/soma/*` contains product behavior, policies, routes, tools, commands, and adapters specific to Soma.

The core operational rule remains:

```text
CLI command ─┐
REST route ──┼──▶ soma-application use case
MCP tool ────┘
```

A surface translates input and output. It does not independently implement the business operation.

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
│   └── palette/                          # existing Palette application/source
│
└── crates/
    ├── shared/
    │   ├── traces/
    │   ├── openapi/
    │   ├── codemode/
    │   ├── mcp-client/
    │   ├── mcp-server/
    │   ├── mcp-proxy/
    │   ├── gateway/
    │   ├── provider-kit/
    │   ├── provider-adapters/
    │   ├── web-kit/
    │   ├── cli-kit/
    │   └── codex-app-server-client/
    │
    └── soma/
        ├── domain/
        ├── application/
        ├── config/
        ├── client/
        ├── integrations/
        ├── auth/
        ├── observability/
        ├── runtime/
        ├── api/
        ├── mcp/
        ├── cli/
        ├── plugin-support/
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

A crate belongs in `crates/soma` when it defines or implements Soma-specific behavior, even when several Soma binaries or surfaces reuse it.

---

## 2. Directory names, package names, and Rust import names

The physical path determines the architectural layer. The package name determines Cargo identity. The Rust crate name is the package name with hyphens converted to underscores.

| Path | Cargo package | Rust import | Classification |
|---|---|---|---|
| `apps/soma` | `soma` | `soma` | executable composition |
| `crates/shared/traces` | `rmcp-traces` | `rmcp_traces` | shared |
| `crates/shared/openapi` | `soma-openapi` | `soma_openapi` | shared |
| `crates/shared/codemode` | `soma-codemode` | `soma_codemode` | shared |
| `crates/shared/mcp-client` | `soma-mcp-client` | `soma_mcp_client` | shared |
| `crates/shared/mcp-server` | `soma-mcp-server` | `soma_mcp_server` | shared |
| `crates/shared/mcp-proxy` | `soma-mcp-proxy` | `soma_mcp_proxy` | shared |
| `crates/shared/gateway` | `soma-gateway` | `soma_gateway` | shared |
| `crates/shared/provider-kit` | `soma-provider-kit` | `soma_provider_kit` | shared |
| `crates/shared/provider-adapters` | `soma-provider-adapters` | `soma_provider_adapters` | shared |
| `crates/shared/web-kit` | `soma-web-kit` | `soma_web_kit` | shared |
| `crates/shared/cli-kit` | `soma-cli-kit` | `soma_cli_kit` | shared |
| `crates/shared/codex-app-server-client` | `codex-app-server-client` | `codex_app_server_client` | shared |
| `crates/soma/domain` | `soma-domain` | `soma_domain` | product |
| `crates/soma/application` | `soma-application` | `soma_application` | product |
| `crates/soma/config` | `soma-config` | `soma_config` | product |
| `crates/soma/client` | `soma-client` | `soma_client` | product |
| `crates/soma/integrations` | `soma-integrations` | `soma_integrations` | product |
| `crates/soma/auth` | `soma-auth` | `soma_auth` | product |
| `crates/soma/observability` | `soma-observability` | `soma_observability` | product |
| `crates/soma/runtime` | `soma-runtime` | `soma_runtime` | product |
| `crates/soma/api` | `soma-api` | `soma_api` | product |
| `crates/soma/mcp` | `soma-mcp` | `soma_mcp` | product |
| `crates/soma/cli` | `soma-cli` | `soma_cli` | product |
| `crates/soma/plugin-support` | `soma-plugin-support` | `soma_plugin_support` | product |
| `crates/soma/test-support` | `soma-test-support` | `soma_test_support` | product |
| `crates/soma/web` | `soma-web` | `soma_web` | product |

The `soma-` prefix on a shared package is branding and namespace protection. It does not make the crate product-specific. The path and dependency rules are the source of truth.

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
    auth routes, observability routes, and soma_web fallback.
    Call soma_web_kit::serve(...).

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

## 3.3 `crates/shared/openapi`: reusable OpenAPI engine

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

A reusable provider adapter may also project indexed OpenAPI operations into `soma-provider-kit`:

```text
soma-provider-adapters::openapi
    depends on soma-openapi + soma-provider-kit
```

Do not create a second OpenAPI executor inside `provider-adapters`, `soma-application`, or `soma-gateway`.

---

## 3.4 `crates/shared/codemode`: reusable Code Mode runtime

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

## 3.5 `crates/shared/gateway`: reusable upstream MCP gateway

**Package:** `soma-gateway`

Suggested layout:

```text
crates/shared/gateway/src/
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

- `soma-auth` types
- Soma scopes or principals
- Soma REST routes
- Soma MCP tool descriptions
- Soma URI schemes such as `soma://upstream/`
- `SOMA_HOME`, `.soma`, or other Soma configuration defaults
- product-level confirmation policy
- product audit policy
- product configuration file layout

### Depends on

```text
soma-gateway ─────────────▶ soma-mcp-proxy
soma-mcp-proxy ───────────▶ soma-mcp-client
soma-mcp-proxy ───────────▶ soma-mcp-server
```

The gateway is the reusable engine that users instantiate when they want a full MCP aggregation runtime. It is not the primitive client or server library. A project that only needs to call upstream MCP servers depends on `soma-mcp-client`; a project that only needs to expose an MCP server depends on `soma-mcp-server`; a project that needs to bridge inbound MCP requests to upstream servers depends on `soma-mcp-proxy`.

### Critical boundary

This dependency is forbidden:

```text
crates/shared/gateway ──X──▶ crates/soma/auth
```

The bridge belongs here:

```text
crates/soma/integrations/src/gateway_auth.rs
```

That bridge can depend on both `soma-gateway` and `soma-auth` and implement the generic gateway auth hook.

The reviewed gateway branch currently has these Soma-shaped pieces that must be neutralized before `soma-gateway` is declared shared:

- `soma-auth` optional dependency and direct `soma_auth::upstream::*` runtime/cache/manager types.
- `SOMA_HOME`, `.soma`, and `soma_home` validation in gateway path defaults.
- default protected-route scopes of `soma:read` and `soma:write`.
- `soma.gateway.error.v1` as the structured error schema namespace.
- `soma://upstream/` as the upstream resource URI prefix.
- special-case routing for a native tool named `soma`.
- error/remediation text that says "start Soma" instead of naming the host application.

Replace these with explicit gateway configuration, generic policy traits, or product adapters under `crates/soma/integrations`. If a separate product wrapper becomes useful, create it under `crates/soma/gateway`; do not let product defaults live in `crates/shared/gateway`.

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

## 3.6 `crates/shared/provider-kit`: reusable provider framework

**Package:** `soma-provider-kit`

Suggested layout:

```text
crates/shared/provider-kit/src/
├── lib.rs
├── id.rs
├── manifest.rs
├── schema.rs
├── validation.rs
├── capability.rs
├── action.rs
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
- action/tool, prompt, resource, task, and elicitation metadata
- provider registration
- immutable snapshots and fingerprints
- indexes for action names and surface overlays
- generic dispatch
- generic provider errors

### Does not own

- Soma authorization policy
- Soma configuration
- Soma built-in commands
- transport-specific HTTP, MCP, or CLI DTOs
- process startup
- a concrete OpenAPI engine
- a concrete Code Mode engine
- an upstream MCP gateway implementation

### Relationship to gateway

These are distinct bounded contexts:

```text
soma-provider-kit
    In-process provider capability registry and projection model.

soma-gateway
    Upstream MCP topology, connections, routing, sessions, and lifecycle.
```

A gateway adapter can project gateway capabilities into the provider registry, but the registries should not be fused.

---

## 3.7 `crates/shared/provider-adapters`: reusable provider implementations

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

Reusable implementations of `soma-provider-kit` contracts, including feature-gated bridges to other shared engines.

Examples:

```text
openapi adapter
    soma-provider-kit + soma-openapi

codemode adapter
    soma-provider-kit + soma-codemode

gateway adapter
    soma-provider-kit + soma-gateway

WASM/Python/TypeScript adapters
    soma-provider-kit + their generic runtimes
```

### Product-specific exception

A provider that calls back into Soma's own product API, uses Soma auth types, or depends on Soma runtime state belongs in `crates/soma/integrations`, not here.

### Do not over-split

Start with one feature-gated `provider-adapters` crate. Create one crate per adapter only when independent versioning, dependency weight, or ownership makes the split valuable.

---

## 3.8 MCP role crates: client, server, and proxy

Split reusable MCP infrastructure by protocol role. This keeps each consumer's dependency surface honest:

```text
Want to call upstream MCP servers?        depend on soma-mcp-client
Want to expose an MCP server?             depend on soma-mcp-server
Want to bridge inbound to upstream MCP?   depend on soma-mcp-proxy
Want the full aggregation gateway?        depend on soma-gateway
```

The role crates are shared infrastructure. None may import Soma product crates, Soma scopes, Soma config defaults, or Soma tool schemas.

### `crates/shared/mcp-client`

**Package:** `soma-mcp-client`

Suggested layout:

```text
crates/shared/mcp-client/src/
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

Owns outbound MCP client sessions, upstream discovery, tool/resource/prompt calls, stdio process lifecycle, HTTP/SSE/WebSocket client transports, response caps, upstream health, and client-side security checks such as SSRF and environment redaction.

Does not own inbound `ServerHandler` implementations, route aggregation, protected public routes, or gateway administration.

### `crates/shared/mcp-server`

**Package:** `soma-mcp-server`

Suggested layout:

```text
crates/shared/mcp-server/src/
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

Owns reusable inbound RMCP server lifecycle helpers, stdio and HTTP serving helpers, cancellation/shutdown integration, response page storage, protocol conversion helpers, conformance-test helpers, and integration with `rmcp-traces`.

Does not own Soma tools, prompts, resources, scopes, product action dispatch, or product-specific MCP error messages.

### `crates/shared/mcp-proxy`

**Package:** `soma-mcp-proxy`

Suggested layout:

```text
crates/shared/mcp-proxy/src/
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

---

## 3.9 `crates/shared/web-kit`: reusable web-server plumbing

**Package:** `soma-web-kit`

Suggested layout:

```text
crates/shared/web-kit/src/
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
- reusable health/readiness primitives
- generic rejection and error-envelope helpers
- reusable SSE, WebSocket, and static-file helpers when proven reusable

### Does not own

- `/v1/*` Soma routes
- product auth policy
- Soma OpenAPI document content
- embedded Soma UI assets
- action dispatch

### `web-kit` versus `api-kit`

Do not create a separate `api-kit` initially. Put genuinely reusable JSON API helpers into modules inside `web-kit`. Split `api-kit` later only when it has a distinct consumer and dependency boundary.

---

## 3.10 `crates/shared/cli-kit`: reusable CLI plumbing

**Package:** `soma-cli-kit`

Suggested layout:

```text
crates/shared/cli-kit/src/
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

## 3.11 `crates/shared/codex-app-server-client`

**Package:** `codex-app-server-client`

Keep the current standalone client here.

### Owns

- the typed Codex app-server protocol client
- transport and protocol DTOs specific to that external server
- no Soma product behavior

It may later be consumed by Code Mode, gateway, provider adapters, or Soma integrations depending on the actual use case. Do not move it into `crates/soma` merely because Soma currently consumes it.

---

## 3.12 `crates/soma/domain`: product concepts and invariant rules

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

## 3.13 `crates/soma/application`: shared product use cases

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

## 3.14 `crates/soma/config`: product configuration

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

## 3.15 `crates/soma/client`: Soma upstream client

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

## 3.16 `crates/soma/integrations`: product adapters to shared engines

**Package:** `soma-integrations`

Suggested layout:

```text
crates/soma/integrations/src/
├── lib.rs
├── provider_runtime.rs
├── gateway.rs
├── gateway_auth.rs
├── gateway_trace.rs
├── codemode.rs
├── openapi.rs
├── upstream.rs
├── remote_provider.rs
├── audit.rs
└── error.rs
```

This crate answers the earlier adapter question precisely. The adapter connecting the standalone gateway to `soma-auth` goes here, not under `apps/soma` and not inside `soma-gateway`.

### Owns

- implementations of `soma-application` ports
- translation between Soma product types and shared-engine types
- gateway-to-auth bridge
- gateway trace propagation bridge
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
├── soma-client
├── soma-provider-kit
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

## 3.17 `crates/soma/auth`: Soma authentication and authorization implementation

**Package:** `soma-auth`

Suggested layout remains close to its current shape.

### Owns

- Soma principals and credential processing where product-specific
- bearer-token policy
- OAuth/JWT product implementation
- scope resolution
- implementations of application authorization ports
- product auth routes or route builders if they cannot be generic

### Does not own

- generic gateway auth interfaces
- action workflows
- API business routes
- process composition

The shared gateway defines an auth hook. `soma-integrations::gateway_auth` implements that hook with `soma-auth`.

---

## 3.18 `crates/soma/observability`

**Package:** `soma-observability`

### Owns

- Soma metric names
- product dashboards/labels
- tracing initialization presets
- product audit/telemetry sinks
- readiness and health observation helpers

Generic request tracing middleware belongs in `web-kit`. RMCP trace parsing belongs in `rmcp-traces`. Product telemetry policy belongs here.

---

## 3.19 `crates/soma/runtime`: initialized product runtime

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

## 3.20 `crates/soma/api`: Soma HTTP adapter

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

### Does not own

- action execution workflows
- direct provider registry dispatch
- construction of `SomaClient` or shared engines
- listener binding
- generic middleware

Target state:

```rust
pub struct ApiState {
    pub application: Arc<SomaApplication>,
}
```

---

## 3.21 `crates/soma/mcp`: Soma MCP adapter

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

## 3.22 `crates/soma/cli`: Soma CLI adapter

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

- `soma-cli-kit` for generic output, terminal, confirmation, and completion helpers

### Does not own

- direct construction of `SomaClient`, `SomaService`, or provider registry
- provider refresh policy
- local/remote business behavior
- action execution

The CLI may collect confirmation, but the application validates the confirmation requirement.

---

## 3.23 Remaining Soma product crates

### `crates/soma/plugin-support`

Keep Soma plugin packaging, setup, metadata projection, and product plugin behavior here. Extract a generic plugin kit only after another unrelated product consumes the same abstraction.

### `crates/soma/test-support`

Own product fixtures, fake application ports, contract snapshot helpers, process fixtures, and test configuration.

### `crates/soma/web`

Own the Rust crate that embeds or serves Soma's compiled frontend assets and product-specific fallback router.

This is distinct from:

```text
apps/web
    editable frontend source/application

crates/soma/web
    Rust-side product integration and embedded assets

crates/shared/web-kit
    generic server and middleware helpers
```

---

## 4. Dependency graph

## 4.1 Target graph

```text
                                      apps/soma
                                         │
              ┌──────────────────────────┼──────────────────────────┐
              │                          │                          │
              ▼                          ▼                          ▼
          soma-cli                   soma-api                   soma-mcp
              │                          │                          │
              ▼                          ▼                          ▼
         soma-cli-kit              soma-web-kit              soma-mcp-server
                                                                     │
                                                                     ▼
                                                                 rmcp-traces

              soma-cli ───────┐
              soma-api ───────┼──────────────▶ soma-application
              soma-mcp ───────┘                       │
                                                     ▼
                                                soma-domain
                                                     ▲
                                                     │
                                             soma-integrations
        ┌────────────────────────────────────────────┼──────────────────────────────┐
        │                  │                         │                 │            │
        ▼                  ▼                         ▼                 ▼            ▼
   soma-client        soma-auth          soma-provider-kit      soma-gateway  soma-codemode
                                                  │                 │             │
                                                  ▼                 ▼             ▼
                                      soma-provider-adapters  soma-mcp-proxy  soma-openapi
                                                                    │
                                                                    ▼
                                                             soma-mcp-client

                                  soma-runtime
                                      │
                                      ▼
                              soma-application
```

`apps/soma` also depends on configuration, observability, runtime, web, and plugin support as required by features.

## 4.2 Mandatory dependency rules

### Shared layer

```text
crates/shared/*
    may depend on external crates
    may depend on crates/shared/*
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
    may depend on neutral shared contracts such as soma-provider-kit
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
soma-mcp
soma-cli
    depend on soma-application
    depend on their respective shared kit or MCP role crate
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
    is the only package expected to depend broadly across the graph
    owns composition, not business policy
```

## 4.3 Shared-layer DAG

Recommended direction:

```text
rmcp-traces                         leaf
soma-openapi                        leaf
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

soma-provider-adapters ───────▶ soma-provider-kit
soma-provider-adapters ───────▶ soma-openapi           optional
soma-provider-adapters ───────▶ soma-codemode          optional
soma-provider-adapters ───────▶ soma-gateway           optional

soma-web-kit                        independent
soma-cli-kit                        independent
```

Do not introduce cycles among shared crates. If gateway and provider adapters need each other in both directions, extract the shared contract or keep one direction through an adapter owned by the higher layer.

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

crates/shared/gateway
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
| `crates/soma-openapi` | `crates/shared/openapi` | `soma-openapi` |
| `crates/soma-codemode` | `crates/shared/codemode` | `soma-codemode` |
| `crates/soma-mcp-client` | `crates/shared/mcp-client` | `soma-mcp-client` |
| `crates/soma-mcp-server` | `crates/shared/mcp-server` | `soma-mcp-server` |
| `crates/soma-mcp-proxy` | `crates/shared/mcp-proxy` | `soma-mcp-proxy` |
| `crates/soma-gateway` | `crates/shared/gateway` | `soma-gateway` |
| `crates/codex-app-server-client` | `crates/shared/codex-app-server-client` | unchanged |
| `crates/soma-api` | `crates/soma/api` | `soma-api` |
| `crates/soma-auth` | `crates/soma/auth` | `soma-auth` |
| `crates/soma-cli` | `crates/soma/cli` | `soma-cli` |
| `crates/soma-mcp` | `crates/soma/mcp` | `soma-mcp` |
| `crates/soma-observability` | `crates/soma/observability` | unchanged |
| `crates/soma-plugin-support` | `crates/soma/plugin-support` | unchanged |
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
    → soma-provider-kit

capabilities.rs and generic provider errors
    → soma-provider-kit

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
    → soma-provider-kit

actions.rs
    split by ownership:
        generic provider/action metadata → soma-provider-kit
        product use-case request/results → soma-application
        invariant product values → soma-domain
        API DTOs → soma-api
        MCP DTOs → soma-mcp
        CLI DTOs → soma-cli

token_limit.rs
    product response policy → soma-application
    generic byte/token helper, only if reusable → appropriate shared kit

errors.rs
    split by the layer that creates each error
```

### From current runtime and app package

```text
crates/soma/src/routes.rs
    → apps/soma/src/http.rs for composition
    → soma-api for product REST routes
    → soma-mcp for product HTTP MCP adapter
    → soma-web-kit for generic listener/middleware

crates/soma/src/runtime.rs
    → apps/soma/src/bootstrap.rs for construction
    → apps/soma/src/local.rs for one-shot CLI mode
    → apps/soma/src/stdio.rs for stdio mode
    → soma-runtime for task supervision/readiness

soma-runtime AppState fields
    SomaService + ProviderRegistry
    → Arc<SomaApplication>
```

---

## 7. Workspace manifest

Recommended root structure:

```toml
[workspace]
resolver = "2"
members = [
    "apps/soma",
    "crates/shared/*",
    "crates/soma/*",
    "xtask",
]
```

Keep non-Rust frontend directories such as `apps/web` and `apps/palette` outside Cargo membership unless they contain Rust packages.

Centralize all internal paths:

```toml
[workspace.dependencies]
# Shared
rmcp-traces = { path = "crates/shared/traces" }
soma-openapi = { path = "crates/shared/openapi" }
soma-codemode = { path = "crates/shared/codemode" }
soma-mcp-client = { path = "crates/shared/mcp-client" }
soma-mcp-server = { path = "crates/shared/mcp-server" }
soma-mcp-proxy = { path = "crates/shared/mcp-proxy" }
soma-gateway = { path = "crates/shared/gateway" }
soma-provider-kit = { path = "crates/shared/provider-kit" }
soma-provider-adapters = { path = "crates/shared/provider-adapters" }
soma-web-kit = { path = "crates/shared/web-kit" }
soma-cli-kit = { path = "crates/shared/cli-kit" }
codex-app-server-client = { path = "crates/shared/codex-app-server-client" }

# Soma product
soma-domain = { path = "crates/soma/domain" }
soma-application = { path = "crates/soma/application" }
soma-config = { path = "crates/soma/config" }
soma-client = { path = "crates/soma/client" }
soma-integrations = { path = "crates/soma/integrations" }
soma-auth = { path = "crates/soma/auth" }
soma-observability = { path = "crates/soma/observability" }
soma-runtime = { path = "crates/soma/runtime" }
soma-api = { path = "crates/soma/api" }
soma-mcp = { path = "crates/soma/mcp" }
soma-cli = { path = "crates/soma/cli" }
soma-plugin-support = { path = "crates/soma/plugin-support" }
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

cli = ["dep:soma-cli", "dep:soma-cli-kit"]
mcp = ["dep:soma-mcp"]
mcp-stdio = ["mcp"]
mcp-http = ["mcp", "api"]
api = ["dep:soma-api", "dep:soma-web-kit"]
auth = ["dep:soma-auth"]
oauth = ["auth"]
web = ["api", "dep:soma-web"]
observability = ["dep:soma-observability"]
plugin = ["dep:soma-plugin-support"]
gateway = ["dep:soma-gateway", "soma-integrations/gateway"]
codemode = ["dep:soma-codemode", "soma-integrations/codemode"]
openapi = ["dep:soma-openapi", "soma-integrations/openapi"]

local-adapter = ["cli", "mcp-stdio"]
server = ["cli", "api", "mcp-http", "mcp-stdio"]
full = [
    "server",
    "auth",
    "oauth",
    "web",
    "observability",
    "plugin",
    "gateway",
    "codemode",
    "openapi",
]
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
- remove `soma-gateway -> soma-auth`
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

git mv crates/soma apps/soma
mkdir -p crates/soma

# Incoming standalone crates
git mv crates/rmcp-traces crates/shared/traces
git mv crates/soma-openapi crates/shared/openapi
git mv crates/soma-codemode crates/shared/codemode
git mv crates/soma-mcp-client crates/shared/mcp-client
git mv crates/soma-mcp-server crates/shared/mcp-server
git mv crates/soma-mcp-proxy crates/shared/mcp-proxy
git mv crates/soma-gateway crates/shared/gateway

# Existing standalone client
git mv crates/codex-app-server-client crates/shared/codex-app-server-client

# Existing Soma product crates
git mv crates/soma-api crates/soma/api
git mv crates/soma-auth crates/soma/auth
git mv crates/soma-cli crates/soma/cli
git mv crates/soma-contracts crates/soma/contracts
git mv crates/soma-mcp crates/soma/mcp
git mv crates/soma-observability crates/soma/observability
git mv crates/soma-plugin-support crates/soma/plugin-support
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

1. `crates/shared/*` has no dependency on `crates/soma/*` or `apps/*`.
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

## PR 9: Extract `soma-provider-kit`

### Goal

Move the generic provider model and registry out of legacy product service/contracts.

### Move

- provider manifests
- provider validation
- capabilities
- action/tool/prompt/resource/task/elicitation metadata
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
cargo test -p soma-provider-kit --all-features
cargo tree -p soma-provider-kit --all-features
```

The dependency graph is shared-only.

---

## PR 10: Extract reusable provider adapters and reconcile engines

### Goal

Create one adapter layer instead of parallel OpenAPI, Code Mode, gateway, and provider implementations.

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
    projects gateway catalog/routes through soma-provider-kit
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

### Move

```text
business workflows
    → soma-application

invariant rules
    → soma-domain

remote Soma HTTP client
    → soma-client

generic provider framework
    → soma-provider-kit

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

### Move by ownership

```text
configuration/environment
    → soma-config

provider manifests and validation
    → soma-provider-kit

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

## PR 15: Extract `soma-web-kit`

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

A fake unrelated Axum router can be served through `soma-web-kit`.

---

## PR 16: Extract `soma-cli-kit`

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

## PR 17: Slim `apps/soma` and finalize composition

### Goal

Make the binary package an unmistakable composition root.

### Finalize

- `bootstrap.rs` builds concrete graph
- `http.rs` composes product routers and calls `web-kit`
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

## PR 18: Delete legacy facades and update ecosystem artifacts

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
  depends on product package soma-auth
  edge: soma-gateway --feature oauth -> soma-auth

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
cargo tree -p soma-openapi --all-features
cargo tree -p soma-codemode --all-features
cargo tree -p soma-gateway --all-features
cargo tree -p soma-provider-kit --all-features
cargo tree -p soma-provider-adapters --all-features
cargo tree -p soma-mcp-client --all-features
cargo tree -p soma-mcp-server --all-features
cargo tree -p soma-mcp-proxy --all-features
cargo tree -p soma-web-kit --all-features
cargo tree -p soma-cli-kit --all-features
```

Fail CI when any tree reaches `crates/soma` or `apps/soma`.

---

## 12. Test strategy

## 12.1 Shared-crate tests

Every shared crate gets:

- direct unit tests
- at least one explicit-construction integration test
- no-default-features build when supported
- all-features build
- dependency-boundary check
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

- executable Rust product package is under `apps/soma`
- every cross-project crate is under `crates/shared`
- every Soma product library is under `crates/soma`
- package names remain stable unless a separate naming decision changes them

### Shared boundaries

- no shared crate depends on a product crate under any feature
- gateway has no `soma-auth` dependency
- Code Mode and OpenAPI have one canonical implementation each
- shared crates construct from explicit configuration

### Business boundary

- CLI, API, and MCP all call `SomaApplication`
- shared business operations live in `soma-application`
- invariant product rules live in `soma-domain`
- concrete engine bridges live in `soma-integrations`
- surfaces contain no direct provider/gateway/OpenAPI/Code Mode dispatch

### Runtime/composition

- runtime exposes application and supervised runtime handles, not a sack of internals
- app package owns construction, mode selection, listeners, and shutdown
- app package does not own business policy

### Legacy removal

- `soma-service` is removed
- `soma-contracts` is removed
- architecture checker has no temporary exceptions

### Behavior

- CLI, REST, MCP, OpenAPI, provider docs, plugins, auth, and feature profiles preserve their documented contracts unless a separate change explicitly versions them

---

## 14. Recommended immediate execution order

The first four moves should be:

1. **Fix and merge the standalone foundations.** Align RMCP, remove gateway-to-auth, and eliminate duplicated OpenAPI/Code Mode engines.
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
| Soma auth implementation | `crates/soma/auth` |
| initialized tasks/readiness | `crates/soma/runtime` |
| Soma REST routes | `crates/soma/api` |
| Soma MCP tools/prompts/resources | `crates/soma/mcp` |
| Soma CLI commands | `crates/soma/cli` |
| generic trace metadata | `crates/shared/traces` |
| generic OpenAPI engine | `crates/shared/openapi` |
| generic Code Mode runtime | `crates/shared/codemode` |
| generic outbound MCP client | `crates/shared/mcp-client` |
| generic inbound MCP server helpers | `crates/shared/mcp-server` |
| generic MCP inbound-to-upstream proxy | `crates/shared/mcp-proxy` |
| reusable MCP gateway engine | `crates/shared/gateway` |
| generic provider registry/contracts | `crates/shared/provider-kit` |
| generic concrete provider implementations | `crates/shared/provider-adapters` |
| generic Axum lifecycle/middleware | `crates/shared/web-kit` |
| generic terminal/CLI helpers | `crates/shared/cli-kit` |
| typed Codex app-server client | `crates/shared/codex-app-server-client` |
| gateway plus Soma auth adapter | `crates/soma/integrations/gateway_auth.rs` |
