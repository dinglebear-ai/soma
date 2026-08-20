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
- Stdio runtimes are allowlisted by bare executable name and their argv is
  runtime-guarded before spawn. Node/npx/Bun inline execution, preload/debug
  flags, Docker host-access flags, Python inline/stdin execution, Deno blanket
  permissions/eval, and argv control characters must fail closed. Upstream env
  must not replace trusted process-discovery/runtime controls such as `PATH`,
  dynamic-loader variables, `NODE_OPTIONS`, `PYTHONPATH`, or `PYTHONHOME`, and
  env values may not contain line/NULL separators. Extra allowlisted commands
  remain subject to the generic argv/env policy even when they have no built-in
  runtime-specific flag set.
- Shared 2026-07-28 upstream connections may open one generation-bound
  `subscriptions/listen` stream. The client normalizes tool/prompt/resource list
  changes and resource updates onto a bounded broadcast bus; legacy protocol
  versions are never sent the modern listen request. Subscription filters mirror
  proxy/exposure policy, list-change refreshes are bounded and exact-peer-safe,
  visible resource-URI changes replace the listen generation, and replacing an
  upstream config cancels its prior generation.
- Streamable HTTP is the final SEP-2243 wire boundary. `Mcp-Method` and
  `Mcp-Name` must be derived from the exact JSON-RPC body there, while RMCP's
  schema-derived `Mcp-Param-*` headers pass through unchanged.
- A typed `HEADER_MISMATCH` from `tools/call` may trigger exactly one bounded
  `tools/list` cache refresh and one replay. Recovery uses the configured
  tools-list response cap; a second mismatch is returned to the caller.
- A completed `tools/call` with `isError: true` is a tool-execution failure, not
  a successful structured value and not a transport failure. Plain calls map it
  to `UpstreamError::ToolExecution`; MRTR calls retain the complete MCP result.
  Upstream-supplied error kinds are untrusted and must be canonicalized before
  they influence caller-facing classification.
- `ToolExecutionAnalysis` is a versioned, transport-neutral recovery contract.
  It may expose bounded cause/kind/retry metadata, advisory safety hints,
  `recovery.action`, `recovery.same_arguments`, guidance, and side-effect
  posture. Do not copy raw upstream content blocks into this analysis: public
  recovery metadata must stay bounded and must not become a secret-exfiltration
  path. Arbitrary unknown `original_kind` strings are not reflected, and product
  surfaces must run `cause` through the normal public-diagnostic redactor before
  exposing details. Safety annotations remain advisory; they never authorize a
  tool call.
