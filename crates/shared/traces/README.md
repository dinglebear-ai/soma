# rmcp-traces

Bounded, log-safe helpers for W3C trace metadata carried through RMCP `_meta`.

This crate targets `rmcp 2.2.0`. RMCP owns wire serialization for `_meta`; this crate adds validation, bounds, trust labels, and redacted summaries.

## V1 Scope

- Parse and validate request-side `traceparent`.
- Preserve bounded `tracestate` and `baggage` privately.
- Produce `TraceSummary` values safe for logs.
- Treat inbound metadata as untrusted by default.
- Keep v00 `traceparent` exact while allowing bounded higher-version additive fields to preserve stable trace/span ID prefixes.

## Non-goals

- No result `_meta` helpers in v1.
- No HTTP propagation in v1.
- No OpenTelemetry SDK/exporter.
- No tracing subscriber setup.
- No auth, gateway, Axum, Tower, reqwest, codemode, or product runtime dependencies.

## Safety

Never log raw baggage. Baggage may contain PII or credentials. `TraceContext` debug formatting delegates to `TraceSummary` and does not print raw baggage values.

Upstream RMCP debug logs can include raw request values before an application receives `RequestContext.meta`. Avoid broad `rmcp=debug` logging for untrusted production traffic.

## Soma Integration

Soma reads `RequestContext.meta` after the MCP auth context is accepted in `crates/soma/mcp/src/rmcp_server.rs`. Auth and pre-auth paging rejections do not include caller-controlled trace fields. Post-auth logs include only `TraceSummary` fields: `trace_id_prefix`, `span_id_prefix`, sampled flag, trust label, tracestate presence, baggage member count, sensitive baggage member count, and safe invalid reasons.

Soma does not attach result `_meta` in v1. This prevents trace metadata from bypassing response paging or `MAX_RESPONSE_BYTES`.

## Future Work

- HTTP propagation behind an app-level trust policy.
- Result `_meta` helpers with one serialized budget across every result path.
- Detailed Lab, Cortex, and Axon migrations after the Soma proof is stable.
