# Trace Context

Soma's MCP surface can bridge inbound W3C `traceparent`, `tracestate`, and
`baggage` metadata into request-scoped trace context from two sources:

1. RMCP `_meta` keys are available on every transport and are untrusted by
   definition. `rmcp-traces` summarizes them into redacted safe fields; raw
   baggage values are never logged.
2. Inbound HTTP headers are considered only when explicitly enabled on a real
   trust boundary. `_meta` always wins: if it carries any trace key, HTTP
   values are never parsed, joined, counted, or logged. Only
   `http_trace_headers_present` and `trace_context_conflict` are recorded.

## `SOMA_MCP_TRACE_HEADERS`

| Value | Behavior |
|---|---|
| `off` (default) | No HTTP trace-header extraction and no HTTP header lookup on the request hot path. |
| `trusted` | Extract validated `traceparent` and `tracestate` after auth; never extract baggage. |
| `trusted-with-baggage` | Also extract validated baggage. Baggage can carry sensitive user, session, or application data; enable deliberately. |

```toml
[mcp]
trace_headers = "trusted"
```

```bash
SOMA_MCP_TRACE_HEADERS=trusted-with-baggage
```

## Trust boundary

Bearer and OAuth authentication are not trace-header trust boundaries. A
valid token does not prove an upstream gateway stripped or overwrote trace
headers supplied by an untrusted client. A non-`off` mode is valid only for:

- `LoopbackDev`, where the loopback bind is the trust boundary.
- `TrustedGatewayUnscoped`, where `SOMA_NOAUTH=true` means an upstream trusted
  gateway enforces authentication and header hygiene before traffic reaches
  Soma.

Mounted bearer and OAuth deployments reject non-`off` modes at startup. A
trusted gateway must strip or overwrite untrusted inbound trace headers before
enabling extraction.

## CORS

CORS is transport permission only; it never establishes trust. The browser
allow-header list is a static exact list built at router construction:

- `off`: no trace headers
- `trusted`: `traceparent`, `tracestate`
- `trusted-with-baggage`: `traceparent`, `tracestate`, `baggage`

There is no wildcard, reflection, or per-request allow-list synthesis.

## Outbound propagation is deferred

This implementation is inbound-only. Inbound trace headers are not forwarded
to Soma's deployed upstream API, the OpenAPI provider adapter, or
gateway-proxied MCP HTTP providers.

`SomaClient` and the OpenAPI adapter accept no inbound trace/header parameter.
The gateway proxy forwards only a fixed allow-list: `accept`, `content-type`,
`mcp-protocol-version`, `mcp-session-id`, and `last-event-id`, plus a separately
resolved upstream bearer token. Regression tests protect both outbound paths.

Attaching Soma's own trace context to outbound calls is a future concern.

## Stdio

Stdio has no HTTP header source. RMCP `_meta` continues to work, while
`SOMA_MCP_TRACE_HEADERS` is inert because request extensions cannot contain
HTTP request parts. Stdio mode uses `AuthPolicy::LoopbackDev` directly and does
not run the HTTP startup trust validation.

## Live smoke

Run:

```bash
cargo xtask test-trace-headers
```

The bounded smoke builds Soma once, starts an isolated local server per mode,
tests real requests and preflights, checks duplicate and non-ASCII header
cases, and asserts that raw baggage never appears in logs. The equivalent thin
wrapper is `scripts/test-trace-headers.sh`.
