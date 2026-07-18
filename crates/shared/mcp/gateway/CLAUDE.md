# soma-gateway

`soma-gateway` is the reusable MCP aggregation gateway runtime crate. It owns
gateway configuration, process safety, upstream transport state, relay/session
behavior, protected-route decisions, adapter seams, dispatch metadata, and
gateway-local view models.

Hard boundaries:

- No `labby-*` dependencies.
- No dependencies on Soma product/runtime/shim crates: `soma`, `soma-runtime`,
  `soma-application`, `soma-domain`, `soma-mcp`, `soma-api`, or `soma-cli`.
- Only optional shared foundation dependencies are allowed. During PR 0 staging
  this means a generic OAuth provider seam behind `oauth`, `soma-codemode`
  behind `codemode`, and `soma-openapi` behind `openapi`; after the physical
  taxonomy PR those move under `crates/shared/*`.
- Product/API/MCP/CLI code parses, delegates, and returns. Gateway runtime logic
  stays in this crate.
- Every new or touched Rust source module gets a sibling `_tests.rs` file.
- No Rust source or test file should exceed 500 physical lines.
- Do not add `mod.rs`.
