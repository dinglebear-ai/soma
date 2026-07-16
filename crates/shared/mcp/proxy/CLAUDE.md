# soma-mcp-proxy

`soma-mcp-proxy` is the reusable MCP proxy helper crate. It owns route
projection for upstream tools, resources, and prompts, including collision
handling and synthetic upstream resource URIs.

Boundary rules:

- No dependency on `soma-gateway` or Soma product/runtime/shim crates.
- Depends downward on `soma-mcp-client` only for shared upstream descriptor
  types.
- Reserved tool names are caller-supplied policy. Do not hard-code Soma product
  tool names here.
- Keep lifecycle, config storage, and admin operations in the gateway layer.
