---
title: "Trace Context"
created: 2026-07-19
updated: 2026-07-30
---

# Trace Context

This is the canonical contract for trace context on Soma's MCP surface. It
covers accepted sources, precedence, trust configuration, validation, logging,
transport behavior, and verification. The reusable parsing API is documented
separately in the [`rmcp-traces` crate README](../crates/shared/traces/README.md).

## Scope

Soma accepts W3C `traceparent`, `tracestate`, and `baggage` metadata from:

1. RMCP request `_meta`, on HTTP and stdio; or
2. inbound HTTP headers, when explicitly enabled at a trusted transport
   boundary.

Only MCP `tools/call` consumes the resulting context. Resource, prompt, tool
listing, and paging operations are unchanged. A valid `traceparent` and
optional `tracestate` are attached to the request-scoped `ExecutionContext`.
Baggage is validated and safely summarized, but its raw content is not placed
in the domain execution context or forwarded downstream.

This feature does not start spans, install an OpenTelemetry exporter, or
propagate context to upstream services.

## Processing order

For an HTTP `tools/call` request, Soma performs the work in this order:

1. Parse and validate response-paging parameters.
2. Authenticate and authorize the MCP request.
3. Resolve trace context from `_meta` or trusted HTTP headers.
4. Build the application `ExecutionContext`.
5. Dispatch the local or gateway tool.
6. Emit a redacted completion or failure summary.

Paging and authentication failures occur before trace extraction. Consequently,
caller-controlled trace values do not appear in their logs.

## Source precedence

RMCP `_meta` always wins. The presence of any `_meta` trace key (`traceparent`,
`tracestate`, or `baggage`) selects `_meta` as the source, even if that value is
invalid. Soma never falls back to HTTP headers after invalid `_meta`.

| `_meta` trace key present | HTTP trace header present | Result |
|---|---|---|
| no | no | No trace context. |
| yes | no | Parse `_meta` as untrusted input. |
| no | yes | Parse HTTP only when the configured mode permits it. |
| yes | yes | Parse `_meta`; do not parse, join, count, or log HTTP values. Record only the safe presence/conflict booleans. |

This precedence prevents a second source from changing a request's trace
identity and prevents losing header values from leaking through validation
errors or debug output.

## Configuration

`mcp.trace_headers` and `SOMA_MCP_TRACE_HEADERS` control only the HTTP source.
They do not disable RMCP `_meta` handling.

| Value | HTTP behavior |
|---|---|
| `off` (default) | Do not look up HTTP trace headers on the request hot path. `_meta` remains available. |
| `trusted` | Extract validated `traceparent` and `tracestate` after auth. Ignore baggage values. |
| `trusted-with-baggage` | Also validate and summarize baggage. Use only when baggage is required and the data policy permits it. |

TOML:

```toml
[mcp]
trace_headers = "trusted"
```

Environment:

```bash
SOMA_MCP_TRACE_HEADERS=trusted
```

Claude plugin setup can map `CLAUDE_PLUGIN_OPTION_TRACE_HEADERS` to the runtime
environment. The generated plugin settings and complete environment registry
are in [PLUGINS.md](PLUGINS.md) and [ENV.md](ENV.md).

## Trust boundary and authentication

Bearer/OAuth authentication is not, by itself, a trace-header trust boundary.
A valid token says nothing about whether a proxy stripped or overwrote headers
supplied by an untrusted client. CORS is also not a trust boundary.

A non-`off` mode is accepted when either:

- Soma binds to loopback; or
- `SOMA_NOAUTH=true` explicitly declares that an upstream trusted gateway
  enforces header hygiene before traffic reaches Soma.

`SOMA_NOAUTH=true` is the trusted-gateway declaration despite its historical
name. Its effect on Soma authentication depends on the rest of the config:

| Deployment | Resulting policy | Trusted HTTP headers allowed? |
|---|---|---|
| Loopback | `LoopbackDev` | yes |
| Non-loopback, bearer token, no trusted-gateway declaration | `MountedBearer` | no |
| Non-loopback, OAuth, no trusted-gateway declaration | `MountedOAuth` | no |
| Non-loopback, trusted gateway, no Soma credential | `TrustedGatewayUnscoped` | yes |
| Non-loopback, trusted gateway plus bearer/OAuth | Mounted bearer/OAuth remains active | yes |

The final row provides defense in depth: explicitly declaring the gateway as
trusted does not remove configured Soma authentication. Without loopback or an
explicit trusted gateway, startup fails with actionable remediation rather
than silently accepting a non-`off` mode.

Before setting `SOMA_NOAUTH=true`, configure the gateway to remove or replace
all client-supplied `traceparent`, `tracestate`, and `baggage` fields. Appending
another value is insufficient because duplicate `traceparent` is rejected and
split optional headers are joined.

## Validation and limits

Soma uses `rmcp-traces` defaults:

| Field | Limit and behavior |
|---|---|
| `traceparent` | 512-byte input bound; W3C version/format, non-zero trace ID, and non-zero span ID validation. Exactly one HTTP field value is required. |
| `tracestate` | 512 bytes, at most 32 unique valid members, and requires a valid `traceparent`. Split HTTP fields are comma-joined within the bound. |
| `baggage` | 8 KiB and at most 64 valid members. Split HTTP fields are comma-joined within the bound. HTTP extraction is opt-in per mode. |

Invalid `traceparent` suppresses the entire HTTP-derived context. Invalid
`tracestate` or baggage is omitted while a valid `traceparent` remains usable.
Invalid values never become tool errors: the tool continues with the valid
subset and the safe reason is recorded in the trace summary.

Higher-version `traceparent` values may contain bounded additive fields. V00
must be exactly 55 characters. Sampling uses the low bit of the trace flags;
reserved flag bits are accepted.

## Logging and privacy

Completion and structured-failure events for local and gateway tools include:

- `trace_id_prefix` and `span_id_prefix` (eight characters each);
- `trace_sampled` and `trace_trust`;
- `has_tracestate`;
- `baggage_member_count` and `sensitive_baggage_member_count`;
- `trace_invalid_count` and value-free `trace_invalid_reasons`;
- `http_trace_headers_present`; and
- `trace_context_conflict`.

Soma never logs the full trace ID, span ID, tracestate, baggage values, or
losing HTTP values. `TraceSummary` and `HttpTraceExtraction` also use redacted
`Debug` implementations.

Baggage remains sensitive even when no key looks sensitive. The sensitive
count is a warning signal based on normalized names such as authorization,
cookie, password, secret, token, API key, private key, and session; it is not a data
loss prevention guarantee.

Avoid broad `rmcp=debug` logging on untrusted production traffic. Upstream RMCP
may log raw protocol input before Soma receives and summarizes `_meta`.

## CORS

CORS controls which browser requests can carry headers; it never establishes
trust. Soma builds a static exact allow-header list at router construction:

- `off`: no trace headers;
- `trusted`: `traceparent`, `tracestate`;
- `trusted-with-baggage`: `traceparent`, `tracestate`, `baggage`.

There is no wildcard, reflection, or per-request allow-list synthesis.

## Outbound non-propagation

This implementation is inbound-only. Inbound trace headers are not forwarded
to Soma's deployed upstream API, the OpenAPI provider adapter, or
gateway-proxied MCP HTTP providers.

`SomaClient` and the OpenAPI adapter accept no inbound trace/header parameter.
The gateway proxy forwards only `accept`, `content-type`,
`mcp-protocol-version`, `mcp-session-id`, and `last-event-id`, plus a separately
resolved upstream bearer token. Regression tests protect both outbound paths.

Attaching Soma's own trace context to outbound calls is future work and must
have a separate trust and baggage policy.

## Stdio

Stdio has no HTTP header source. RMCP `_meta` remains available, while
`SOMA_MCP_TRACE_HEADERS` is inert because request extensions cannot contain
HTTP request parts. Stdio mode uses `AuthPolicy::LoopbackDev` directly and does
not run HTTP startup trust validation.

## Troubleshooting

Use `soma doctor` or `soma setup check` before starting a non-loopback server.
An invalid combination is reported as `invalid_trace_headers_trust` with the
same remediation as startup validation.

Common causes:

| Symptom | Check |
|---|---|
| No HTTP context appears | Confirm the request is `tools/call`, the mode is not `off`, and a valid `traceparent` is present. |
| HTTP headers are present but ignored | Check whether any `_meta` trace key selected the higher-precedence source. |
| Baggage count stays zero | Use `trusted-with-baggage`; `trusted` deliberately ignores baggage. |
| Startup rejects the mode | Bind to loopback, turn the mode off, or configure a header-sanitizing gateway and set `SOMA_NOAUTH=true`. |
| Browser preflight rejects a header | Confirm the configured mode includes that header and the origin is separately allowed. |

## Verification

Crate tests:

```bash
cargo test -p rmcp-traces
cargo test -p rmcp-traces --features http
```

Soma integration tests:

```bash
cargo test -p soma --test mcp_trace_headers --features test-support
```

Bounded live smoke:

```bash
cargo xtask test-trace-headers
# Equivalent wrapper:
scripts/test-trace-headers.sh
```

The smoke builds Soma once, starts an isolated local server per mode, exercises
real Streamable HTTP requests and CORS preflights, checks duplicate and
non-visible-ASCII header cases, verifies `_meta` precedence and authentication
ordering, and asserts that raw baggage never appears in logs.
