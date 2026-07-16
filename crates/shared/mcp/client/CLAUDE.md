# soma-mcp-client

`soma-mcp-client` is the reusable outbound MCP client runtime crate. It owns
upstream configuration, transport setup, stdio process safety, upstream
discovery, response caps, and tool/resource/prompt calls.

Boundary rules:

- No dependencies on gateway or Soma product/runtime/shim crates.
- Default features stay minimal and must not enable OAuth, HTTP server, REST,
  CLI, web, or product integration surfaces.
- The optional `oauth` feature may depend on shared auth support only for
  generic upstream OAuth client behavior.
- Product-specific env prefixes, scopes, tool names, and defaults must be
  supplied by the host application, not hard-coded here.
