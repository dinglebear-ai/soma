# soma-gateway

`soma-gateway` is the self-contained Soma gateway runtime crate. It owns gateway
configuration, process safety, upstream transport state, relay/session behavior,
protected-route decisions, adapter seams, dispatch metadata, and gateway-local
view models.

Hard boundaries:

- No `labby-*` dependencies.
- No dependencies on Soma product/runtime/shim crates: `soma`, `soma-runtime`,
  `soma-service`, `soma-contracts`, `soma-mcp`, `soma-api`, or `soma-cli`.
- Only optional leaf Soma dependencies are allowed: `soma-auth` behind `oauth`,
  `soma-codemode` behind `codemode`, and `soma-openapi` behind `openapi` once
  those leaf crates exist.
- Product/API/MCP/CLI code parses, delegates, and returns. Gateway runtime logic
  stays in this crate.
- Every new or touched Rust source module gets a sibling `_tests.rs` file.
- No Rust source or test file should exceed 500 physical lines.
- Do not add `mod.rs`.
