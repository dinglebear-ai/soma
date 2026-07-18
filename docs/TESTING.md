---
title: "Testing"
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

# Testing

The test strategy is layered: parse at the CLI layer, test business/service
behavior without a server, verify static contracts, test REST-client behavior
against local mock upstreams, then smoke-test live MCP HTTP with mcporter when
transport behavior matters.

## Rust tests

```bash
cargo nextest run
cargo nextest run --profile ci
cargo test
just test-ci
```

All repos use `cargo nextest` instead of `cargo test`. Configure in `.config/nextest.toml`:

```toml
[profile.default]
fail-fast = false

[profile.ci]
fail-fast = true
retries = 2
```

## Key test files

| File | Purpose |
|---|---|
| `apps/soma/tests/cli_parse.rs` | CLI parser behavior. |
| `apps/soma/tests/tool_dispatch.rs` | Service/action semantics without live credentials. |
| `apps/soma/tests/api_routes.rs` | REST and mounted auth route behavior. |
| `apps/soma/tests/plugin_contract.rs` | Plugin package and hook contracts. |
| `apps/soma/tests/soma_invariants.rs` | Automation/Soma invariants. |
| `crates/soma/application/src/service_tests.rs` | Private service-layer unit tests (sidecar to `service.rs`). |

## Test sidecars

All tests that need access to private functions live in `_tests.rs` sidecar files, not inline:

```rust
// crates/soma/application/src/service.rs
pub struct SomaService { ... }
impl SomaService { ... }

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

// crates/soma/application/src/service_tests.rs
use super::*;  // access to private items

#[test]
fn destructive_gate_blocks_without_confirm() {
    let svc = SomaService::new(stub_client());
    let err = svc.destructive_gate(false).unwrap_err();
    assert!(err.to_string().contains("confirm=true"));
}

#[test]
fn destructive_gate_allows_with_confirm() {
    let svc = SomaService::new(stub_client());
    assert!(svc.destructive_gate(true).is_ok());
}
```

## Test helpers

`apps/soma/src/lib.rs` exports helpers for integration tests. Prefer the helper over
hand-constructing `AppState` in integration tests:

```rust
use soma::testing::loopback_state;

#[tokio::test]
async fn tool_path_uses_loopback_state() {
    let state = loopback_state();
    assert_eq!(state.config.port, 40060);
}
```

Use `loopback_state()` in integration tests:

```rust
// apps/soma/tests/tool_dispatch.rs
use soma::testing::loopback_state;

#[tokio::test]
async fn help_returns_help_key() {
    let state = loopback_state();
    let result = execute_tool(&state, "soma", json!({"action": "help"})).await.unwrap();
    assert!(result.get("help").is_some());
    assert!(!result["help"].as_str().unwrap().is_empty());
}
```

## Live MCP tests

```bash
just dev
bash apps/soma/tests/mcporter/test-mcp.sh
just test-mcporter
```

The mcporter harness validates tools and resources against a running server. It logs calls to `/tmp/test-mcp.<timestamp>.log`.

The test script validates:
- auth rejection when `SOMA_MCP_TOKEN` is set
- tool semantic behavior for `greet`, `echo`, `status`, and `help`
- MCP resource behavior for `soma://schema/mcp-tool`

Use semantic assertions, not liveness-only checks:

```bash
# Bad test — only proves MCP responded
run_test "server info" "soma" '{"action":"status"}'

# Good test — proves the service actually returned real data
run_test "status has version" "soma" '{"action":"status"}' "version"
```

## Contract-backed REST-client tests

Rust MCP servers that are mostly REST clients should not call real homelab
services in default tests. Use three evidence tiers instead:

| Tier | What it proves | Default? |
|---|---|---|
| `static-spec` | The repo's MCP schema docs, OpenAPI docs, action metadata, plugin contracts, sidecar tests, and Soma invariants are in sync. | Yes |
| `contract-real` | The service builds the expected outbound HTTP requests, parses fixtures, maps upstream errors, and enforces safety gates against a local mock upstream and schema fixtures. | Yes |
| `production-real` | A deployed server can answer read-only MCP calls against a real upstream. | Explicit opt-in only |

Run the static tier with:

```bash
cargo xtask contract-audit
just contract-audit
```

For `contract-real` tests in derived servers, use `wiremock` or an equivalent
local mock server. Assert the outbound method, path, query string, auth header,
and body. Return curated JSON fixtures and validate them with `jsonschema` or
OpenAPI-derived schemas where practical. Keep curated overlays when the upstream
OpenAPI document is incomplete or instance-specific.

Destructive actions must be tested in one of two ways by default:

- without confirmation, assert the service fails before making any network call
- with confirmation, target only a mock server or disposable upstream

Live `mcporter` smoke remains `production-real` evidence. Keep it read-only and
explicitly allowlisted; do not include delete/update/send actions in the live
suite unless a disposable target is configured.

## Soma README Shape checks

```bash
just soma-check
cargo xtask contract-audit
cargo xtask patterns
cargo xtask pre-release-check
```

## Principles

- Assert semantic values, not just valid JSON.
- Assert defaults explicitly.
- Keep business logic tests below HTTP when possible.
- Use live mcporter tests for transport/resource/auth integration.
- Use mock upstream tests for REST-client request construction and response
  parsing; schema-backed mocks are real contract evidence, not production
  health checks.
- A test that checks `is_error: false` only verifies the protocol layer responded — prove the actual data is correct.
- Negative MCP tool tests should assert `isError: true` and inspect the structured
  error payload: `kind`, `schema_version`, stable `code`, `tool`, `action`,
  optional `field`/`bad_value`, and `remediation`. Protocol `ErrorData` should
  be reserved for auth/scope denial, unknown MCP tool names, resource/prompt
  lookup, and server serialization defects.

See `docs/PATTERNS.md` §12, §17, §24, and §51 for test sidecar, mcporter,
nextest, and REST-client contract testing patterns.
