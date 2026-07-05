---
title: "Architecture"
doc_type: "guide"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "template"
source_of_truth: false
upstream_refs:
  - "docs/PATTERNS.md"
last_reviewed: "2026-05-15"
---

# Architecture

`rmcp-template` is a Rust template for MCP servers built on `rmcp`. The architecture is intentionally layered so transports stay thin and business logic stays testable.

## Layer diagram

```
ExampleClient  (crates/rtemplate-service/src/example.rs) → HTTP/API transport ONLY — network calls, no logic
ExampleService (crates/rtemplate-service/src/app.rs)     → ALL business logic, validation, enrichment
ActionRegistry (crates/rtemplate-service/src/actions.rs) → Action metadata, validation, native dispatch
MCP shim       (crates/rtemplate-mcp/src/tools.rs)       → parse JSON args → call service → return Value
CLI shim       (crates/rtemplate-cli/src/lib.rs)         → parse argv → call service → print
REST shim      (crates/rtemplate-api/src/api.rs)       → parse HTTP JSON → call service → return JSON
```

**The golden rule:** If you are writing business logic in `mcp/tools.rs`, `cli.rs`, or `main.rs`, you are doing it wrong. Move it to `app.rs`.

## Module layout

```
crates/
  rmcp-template/        ← thin binary/facade package
    src/main.rs         ← full server binary mode dispatch
    src/bin/example.rs  ← local CLI + stdio MCP binary dispatch
    src/routes.rs       ← axum router: wires mcp + api + auth + SPA fallback
    src/lib.rs          ← public facade + test helpers (testing::*)
    tests/              ← integration tests and mcporter harness
  rtemplate-service/    ← ExampleClient + ExampleService business layer
  rtemplate-contracts/  ← action metadata, config, DTOs, token limits
  rtemplate-api/        ← REST API handlers
  rtemplate-mcp/        ← MCP schemas, tools, prompts, transport
  rtemplate-cli/        ← CLI parser, doctor/setup/watch commands
  rtemplate-runtime/    ← AppState, auth policy, shared runtime wiring
  rtemplate-web/        ← static web asset serving and source bundle helpers
```

## Core files

| File | Responsibility |
|---|---|
| `crates/rtemplate-service/src/example.rs` | Upstream/client transport stub. Replace with your service API client. |
| `crates/rtemplate-service/src/app.rs` | Service layer. All business rules live here. |
| `crates/rtemplate-service/src/actions.rs` | Canonical native action registry, validation, cached catalog/help, and dispatch. |
| `crates/rtemplate-contracts/src/actions.rs` | Shared action metadata types and provider-independent helper functions. |
| `crates/rtemplate-mcp/src/tools.rs` | MCP tool dispatch and elicitation-only actions. |
| `crates/rtemplate-mcp/src/schemas.rs` | Tool input schema generated from action metadata. |
| `crates/rtemplate-mcp/src/rmcp_server.rs` | `ServerHandler`, scope enforcement, tools/resources/prompts. |
| `crates/rtemplate-runtime/src/server.rs` | Shared auth policy resolution and app state. |
| `crates/rmcp-template/src/routes.rs` | HTTP routes for MCP, health, status, REST API, and web assets. |
| `crates/rtemplate-contracts/src/config.rs` | Environment/config loading and safe defaults. |
| `crates/rmcp-template/src/main.rs` | Full server binary mode dispatch. |

## AppState

```rust
#[derive(Clone)]
pub struct AppState {
    pub config: McpConfig,                  // MCP server config (host, port, auth settings)
    pub auth_policy: AuthPolicy,            // LoopbackDev | TrustedGatewayUnscoped | Mounted
    pub service: ExampleService,            // The service layer — everything routes through here
    pub response_pages: ResponsePageStore,  // Cached oversized MCP responses for continuation calls
}
```

`AppState` is cloned per-request by the RMCP framework. Keep it cheap to clone — the service wraps an `Arc`-backed `reqwest::Client` internally.

## Binary and transport profiles

The template supports two deployment shapes, chosen by server category:

| Server kind | Default shape |
|---|---|
| Upstream-client MCP server | Local `CLI + stdio MCP` binary that calls the upstream API directly. |
| Application/platform server | Docker/server binary with REST API, Web UI, Streamable HTTP MCP, health/auth, and optional local CLI/stdio adapter. |

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

REST exposes direct routes only. MCP keeps the single action-dispatched `example`
tool; REST does not mount `/v1/example`.

```rust
// crates/rmcp-template/src/routes.rs
pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health))
        .route("/status", get(status));

    let api = Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .route("/v1/status", get(v1_service_status))
        .route("/v1/help", get(v1_help))
        .route("/v1/{action}", post(v1_action_post))
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

`crates/rtemplate-cli/src/lib.rs` follows the same shim discipline as `crates/rtemplate-mcp/src/tools.rs`. The canonical shape:

```rust
// cli.rs — binary module (uses `example_mcp::` not `crate::`)
use example_mcp::app::ExampleService;

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
            other => bail!("unknown command: {}\n\nRun `example --help`", other.join(" ")),
        };
        Ok((cmd, json))
    }
}

pub async fn run(service: &ExampleService, cmd: CliCommand, json: bool) -> Result<()> {
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
- All action metadata starts in `crates/rtemplate-service/src/actions.rs`; shared contract types stay provider-independent.
- Read actions require `example:read`; write actions require `example:write`; `help` is public.
- Stdio is local trusted transport; HTTP is protected unless in loopback or explicit trusted-gateway mode.
- Plugin setup is binary-owned: hook scripts delegate to `example setup plugin-hook`.

See `docs/PATTERNS.md` §1, §7, §A1, §45 for full pattern details.
