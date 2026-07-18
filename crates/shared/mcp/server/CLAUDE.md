# soma-mcp-server

`soma-mcp-server` is the reusable inbound MCP server helper crate. Its core is
transport-neutral protocol helpers such as response paging and result shaping.
It also owns an optional Streamable HTTP transport module (`http.rs`, behind
the `http` feature) with the deterministic allowed-host/allowed-origin
computation and RMCP transport wiring for inbound HTTP MCP servers.

Boundary rules:

- No dependencies on gateway or Soma product/runtime/shim crates.
- Keep product compatibility knobs explicit, such as action discriminator names
  and response byte caps.
- Do not import product action catalogs, product auth policy, or application
  state.
- Add server lifecycle helpers here only when an unrelated MCP server could use
  them without adopting Soma.
