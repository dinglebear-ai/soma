# soma-mcp-server

`soma-mcp-server` is the reusable inbound MCP server helper crate. It owns
transport-neutral protocol helpers such as response paging and result shaping.

Boundary rules:

- No dependencies on gateway or Soma product/runtime/shim crates.
- Keep product compatibility knobs explicit, such as action discriminator names
  and response byte caps.
- Do not import product action catalogs, product auth policy, or application
  state.
- Add server lifecycle helpers here only when an unrelated MCP server could use
  them without adopting Soma.
