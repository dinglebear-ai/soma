---
title: "ADR 0012: rmcp-traces targets RMCP 2.2.0"
created: 2026-07-15
updated: 2026-07-30
---

# ADR 0012: rmcp-traces targets RMCP 2.2.0

## Status

Accepted

## Context

GitHub issue #76 adds a standalone `rmcp-traces` crate for MCP trace metadata. The compatibility target is `rmcp 2.2.0`, not older planning text.

Live evidence used for this decision:

- `Cargo.lock` resolves `rmcp` and `rmcp-macros` to `2.2.0`.
- Local upstream RMCP source at `/home/jmagar/workspace/upstream/rmcp` was fast-forwarded to `origin/main` commit `92581bee53cd883e5b479616b369c1fcb4eb2fc2`.
- RMCP 2.2.0 exposes `RequestParamsMeta` helpers for request `_meta`, `traceparent`, `tracestate`, and `baggage`.
- RMCP 2.2.0 exposes `Meta` as a transparent JSON object with reserved W3C trace keys.
- RMCP 2.2.0 serializes `CallToolResult::_meta`, but Soma v1 intentionally does not attach result metadata.

## Decision

`rmcp-traces` v1 targets `rmcp 2.2.0` and uses the published RMCP model types directly. The workspace must not compile two incompatible `rmcp::model::Meta` or `rmcp::model::CallToolResult` types.

The v1 crate is request-side only:

- Parse and validate `traceparent`.
- Preserve bounded `tracestate` and `baggage` values in private fields.
- Expose redacted summaries that never print raw baggage.
- Integrate into Soma MCP by summarizing `RequestContext.meta` after MCP auth succeeds in `call_tool`; pre-auth rejection logs omit caller-controlled trace fields.
- Keep v00 `traceparent` exact while preserving stable trace/span ID prefixes from bounded higher-version traceparents with additive fields.

## Non-goals

- No result `_meta` helpers in v1.
- No HTTP propagation in v1.
- No OpenTelemetry SDK or exporter.
- No tracing subscriber setup.
- No auth, gateway, codemode, Axum, Tower, or reqwest dependency.
- No product-specific session IDs or UI widget semantics.
- No automatic Lab, Cortex, Axon, or fleet migration.

## Security Notes

Inbound trace metadata is untrusted unless a host application explicitly marks it trusted. Public caller sampled flags are advisory only. Baggage is a privacy hotspot and must not be logged raw.

Upstream RMCP debug logging can log raw requests before Soma receives `RequestContext.meta`. Production deployments should not enable broad `rmcp=debug` logging for untrusted traffic until that upstream behavior is filtered or changed.

## Consequences

The initial proof is small and reversible. HTTP propagation and result metadata can be added later behind explicit budgets and trust policies.
