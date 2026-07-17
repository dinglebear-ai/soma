---
title: "Architecture"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
upstream_refs:
  - "docs/PATTERNS.md"
last_reviewed: "2026-05-15"
---

# Architecture

`soma` is a Rust product for MCP servers built on `rmcp`. The architecture is intentionally layered so transports stay thin and business logic stays testable.

## Layer diagram

```
SomaClient  (crates/soma/client/src/client.rs) → HTTP/API transport ONLY — network calls, no logic
SomaService (crates/soma/service/src/app.rs)     → ALL business logic, validation, enrichment
MCP shim       (crates/soma/mcp/src/tools.rs)       → parse JSON args → call service → return Value
CLI shim       (crates/soma/cli/src/lib.rs)         → parse argv → call service → print
REST shim      (crates/soma/api/src/api.rs)       → parse HTTP JSON → call service → return JSON
```

**The golden rule:** If you are writing business logic in `mcp/tools.rs`,
`cli.rs`, or the canonical binary entrypoint, you are doing it wrong. Move it to
`app.rs`.

## Module layout

```
apps/
  soma/                    ← thin binary/facade package
    src/bin/soma.rs        ← canonical soma mode dispatch: serve, mcp, CLI
    src/routes.rs          ← axum router: wires mcp + api + auth + SPA fallback
    src/lib.rs             ← public facade + test helpers (testing::*)
    tests/                 ← integration tests and mcporter harness

crates/
  soma/
    api/                   ← REST API handlers
    cli/                   ← CLI parser, doctor/setup/watch commands
    client/                ← SomaClient — HTTP/API transport
    contracts/             ← action metadata, config, DTOs, token limits
    mcp/                   ← Soma-specific MCP schemas, tools, prompts, transport
    runtime/               ← AppState, auth policy, shared runtime wiring
    service/               ← SomaService business layer
    test-support/          ← shared Soma test fixtures and harness helpers
    web/                   ← static web asset serving and source bundle helpers
  shared/
    auth/                  ← reusable bearer/OAuth auth policy and token handling
    codemode/              ← reusable Code Mode runtime and runner support
    mcp/
      client/              ← reusable outbound MCP upstream client runtime
      gateway/             ← reusable MCP aggregation gateway runtime
      proxy/               ← reusable MCP proxy route projection helpers
      server/              ← reusable inbound MCP server protocol helpers
    observability/         ← reusable tracing/metrics wiring
    openapi/               ← reusable OpenAPI operation registry and dispatcher
    traces/                ← reusable RMCP trace capture/support
    codex-app-server-client/
                           ← generated reusable Codex app-server client
```

Shared crates are reusable building blocks below the Soma product layer and must
not depend back on `apps/soma` or `crates/soma/**`. Two pieces sit outside the
client → service → shim pattern:

- `crates/shared/codex-app-server-client/` - a fully-typed async Rust client for the
  Codex CLI's `app-server` v2 JSON-RPC protocol, with zero path-dependencies
  on any other crate in this workspace. See its own README for architecture
  and usage.
- `xtask/` - repo-local build/release tooling (version-sync checks, release
  planning, schema codegen for `codex-app-server-client`). Its
  `codex-schema` subcommand has no dependency on `soma-*` crates, but `xtask`
  itself depends on `soma-domain` and `soma-service` (path deps) for its
  other duties, such as version-sync and release-plan checks. See
  `docs/XTASKS.md`.

## Core files

| File | Responsibility |
|---|---|
| `crates/soma/client/src/client.rs` | Upstream/client transport stub. Replace with your service API client. |
| `crates/soma/service/src/app.rs` | Service layer. All business rules live here. |
| `crates/soma/domain/src/actions.rs` | Canonical action metadata, parsing, REST dispatch helpers. |
| `crates/soma/mcp/src/tools.rs` | MCP tool dispatch and elicitation-only actions. |
| `crates/soma/mcp/src/schemas.rs` | Tool input schema generated from action metadata. |
| `crates/soma/mcp/src/rmcp_server.rs` | `ServerHandler`, scope enforcement, tools/resources/prompts. |
| `crates/soma/runtime/src/server.rs` | Shared auth policy resolution and app state. |
| `apps/soma/src/routes.rs` | HTTP routes for MCP, health, status, REST API, and web assets. |
| `crates/soma/config/src/config.rs` | Environment/config loading and safe defaults. |
| `apps/soma/src/bin/soma.rs` | Canonical binary mode dispatch for `serve`, `mcp`, and CLI commands. |

## AppState

```rust
#[derive(Clone)]
pub struct AppState {
    pub config: McpConfig,                  // MCP server config (host, port, auth settings)
    pub auth_policy: AuthPolicy,            // LoopbackDev | TrustedGatewayUnscoped | Mounted
    pub service: SomaService,            // The service layer — everything routes through here
    pub response_pages: ResponsePageStore,  // Cached oversized MCP responses for continuation calls
}
```

`AppState` is cloned per-request by the RMCP framework. Keep it cheap to clone — the service wraps an `Arc`-backed `reqwest::Client` internally.

## Runtime modes and feature sets

Soma ships one canonical binary with explicit runtime modes. Derived projects
may choose narrower Cargo feature sets, but the command roles stay stable:

| Command | Default shape |
|---|---|
| `soma mcp` | Local stdio MCP adapter. |
| `soma serve` | HTTP runtime with REST API, Web UI, Streamable HTTP MCP, health/auth, and provider registry. |
| `soma <command>` | CLI adapter that uses local provider/static dispatch or the configured remote REST API. |

Keep MCP-specific behavior in the MCP layer. If a stdio adapter talks to a
platform API, that API should expose business actions, not MCP protocol
semantics.

The accepted transport/profile decision is recorded in
[`docs/adr/0001-stdio-first-plugin-adapter.md`](adr/0001-stdio-first-plugin-adapter.md).
The normative plugin adapter checklist lives in
[`docs/contracts/plugin-stdio-adapter.md`](contracts/plugin-stdio-adapter.md).

## Route composition

For the full platform/server profile, HTTP surfaces share one binary on one port:

```
Port 40060
  ├── /mcp                  → Streamable HTTP MCP transport
  ├── /health               → Unauthenticated liveness probe
  ├── /status               → Public redacted runtime state
  ├── /openapi.json         → Public generated REST OpenAPI schema
  ├── /v1/capabilities      → REST route inventory
  ├── /v1/greet             → Direct REST action route
  ├── /v1/echo              → Direct REST action route
  ├── /v1/status            → Direct REST action route
  ├── /v1/help              → Direct REST action route
  ├── /.well-known/*        → OAuth metadata (when auth_mode=oauth)
  └── /*                    → SPA fallback (serves embedded web UI)
```

```rust
// apps/soma/src/routes.rs
pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health))
        .route("/status", get(status));

    let api = Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .route("/v1/greet", post(v1_greet))
        .route("/v1/echo", post(v1_echo))
        .route("/v1/status", get(v1_service_status))
        .route("/v1/help", get(v1_help))
        .route_layer(auth_layer.clone());

    let mcp = Router::new()
        .nest_service("/mcp", streamable_http_service(state.clone(), mcp_config));

    Router::new()
        .merge(public)
        .merge(api)
        .merge(mcp)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
```

## CLI thin shim pattern

`crates/soma/cli/src/lib.rs` follows the same shim discipline as `crates/soma/mcp/src/tools.rs`. The canonical shape:

```rust
// cli.rs — binary module (uses `soma::` not `crate::`)
use soma::app::SomaService;

pub enum CliCommand {
    Things,
    Thing { id: String },
    DeleteThing { id: String, confirm: bool },
}

impl CliCommand {
    pub fn parse(args: &[String]) -> Result<(Self, bool)> {
        let json    = args.iter().any(|a| a == "--json");
        let confirm = args.iter().any(|a| a == "--confirm");
        let rest: Vec<&str> = args.iter()
            .filter(|a| a.as_str() != "--json" && a.as_str() != "--confirm")
            .map(String::as_str).collect();

        let cmd = match rest.as_slice() {
            ["things"]         => Self::Things,
            ["thing", id, ..]  => Self::Thing { id: id.to_string() },
            ["delete", id, ..] => Self::DeleteThing { id: id.to_string(), confirm },
            other => bail!("unknown command: {}\n\nRun `soma --help`", other.join(" ")),
        };
        Ok((cmd, json))
    }
}

pub async fn run(service: &SomaService, cmd: CliCommand, json: bool) -> Result<()> {
    let (label, data) = match cmd {
        CliCommand::Things                            => ("things", service.list_things().await?),
        CliCommand::Thing { ref id }                  => ("thing",  service.get_thing(id).await?),
        CliCommand::DeleteThing { ref id, confirm }   => ("delete", service.delete_thing(id, confirm).await?),
    };
    if json { println!("{}", serde_json::to_string_pretty(&data)?); }
    else    { print_human(label, &data); }
    Ok(())
}
```

`parse()` extracts flags and dispatches to variants — no defaults, no validation, no domain logic. `run()` calls the service and formats output. That's it.

## What "thin shim" means

`mcp/tools.rs` does exactly three things per action:
1. Extract named arguments from the `Value` args object
2. Call the corresponding `state.service.method()`
3. Return the `Value` result

`cli.rs` does exactly three things per command:
1. Parse CLI flags/positional args into typed values
2. Call the corresponding `service.method()`
3. Format and print the result (or pass `--json` through verbatim)

Zero validation, zero defaults, zero error message crafting in shims. All of that lives in `app.rs`.

## Split rules — when to make a directory vs a file

| Surface | Split into a directory when… |
|---|---|
| `<service>/` | upstream API has ≥ 2 resource groups |
| `app/` | service methods exceed one focused domain |
| `api/handlers/` | ≥ 2 resource groups; each file stays thin (≤ 200 lines) |
| `web/pages/` | ≥ 3 page routes |

## File size targets

| Threshold | Action |
|---|---|
| ≤ 250 non-test lines | Target — ideal module size |
| > 400 non-test lines | Must add split/refactor note in PR |
| > 600 non-test lines | Requires documented exception |
| > 800 total lines | Must split unless generated/fixture/schema |

## Modern Rust requirements

- No `mod.rs` files — use named module files (`mcp.rs` + `mcp/tools.rs`)
- Rust 2021 edition minimum, target 2024 where possible
- `thiserror` for structured error types in the service layer
- `?` operator chains over nested `match`
- Avoid `unwrap()`/`expect()` in production paths

## Invariants

- Shims do not contain business logic.
- All action metadata starts in `crates/soma/domain/src/actions.rs`.
- Read actions require `soma:read`; write actions require `soma:write`; `help` is public.
- Stdio is local trusted transport; HTTP is protected unless in loopback or explicit trusted-gateway mode.
- Plugin setup is binary-owned: hook scripts delegate to `soma setup plugin-hook`.

See `docs/PATTERNS.md` §1, §7, §A1, §45 for full pattern details.
