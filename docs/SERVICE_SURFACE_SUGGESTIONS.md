# Service and Surface Suggestions

## Goal

Keep MCP, CLI, REST, and web surfaces thin while letting new business actions live in one shared service path.

## Recommended Shape

Business logic should live in `crates/soma/service`.

The stable flow should be:

```text
MCP / CLI / REST / Web
  -> surface adapter
  -> SomaAction
  -> soma_service::dispatch_action(...)
  -> SomaService method
  -> domain modules or standalone crates
```

`SomaService` in `crates/soma/service/src/app.rs` should be the shared application facade. It can call local modules, upstream clients, repositories, or standalone crates such as auth, codemode, gateway, and traces. Surface crates should parse inputs, call the service, and format outputs.

## MCP Surface

Keep MCP as one action-dispatched tool.

That keeps model context small because clients see one tool with an `action` argument instead of one MCP tool per operation. MCP-only flows such as elicitation can stay in the MCP crate because they need a live MCP peer, but completed business behavior should still delegate back to the service crate.

## REST Surface

REST should use traditional typed routes, not an action envelope.

Good:

```text
POST /v1/echo
GET  /v1/status
GET  /v1/help
```

Avoid:

```text
the retired REST action-envelope route
```

Direct routes give better OpenAPI, clearer auth docs, easier client SDK generation, simpler HTTP debugging, and more natural integration for non-agent consumers.

## Action Contract

Use `crates/soma/contracts/src/actions.rs` as the canonical action contract:

- action enum
- transport availability
- scope requirements
- CLI metadata
- REST method/path metadata
- parameter metadata
- generated help/catalog data

Adding a business action should require one canonical action definition and one service dispatch arm. The surfaces should derive as much as possible from the action contract.

## Automation Opportunities

Soma can make action additions less repetitive by generating or deriving:

- MCP tool schema entries from `ACTION_SPECS`
- CLI parser coverage from `CliSpec`
- REST route inventory from `rest_method` and `rest_path`
- OpenAPI route docs from the action contract
- parity tests that fail when an action is missing from a surface

Direct REST handlers may still stay explicit. That preserves typed request structs and clean OpenAPI while still routing through the same `SomaAction` and `dispatch_action` path.

## Suggested Long-Term Improvement

Introduce a small action-definition macro or builder that declares each action once and generates:

- `SomaAction`
- `ACTION_SPECS`
- MCP argument parsing/schema
- CLI parser metadata
- REST route metadata
- help examples
- parity fixtures

Keep `execute_service_action` explicit:

```rust
match action {
    SomaAction::Echo { message } => service.echo(message).await,
    SomaAction::Status => service.status().await,
    // ...
}
```

That match is useful friction: it makes the service behavior obvious and keeps public action routing reviewable.
