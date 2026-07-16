---
title: "API Reference"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
upstream_refs:
  - "docs/PATTERNS.md"
last_reviewed: "2026-06-17"
---

# API

The server exposes HTTP endpoints alongside MCP when a scaffolded server uses the application/platform profile. All surfaces (MCP, REST, CLI) call the same `SomaService` methods — no logic is duplicated.

Upstream-client MCP servers do not need a local REST mirror by default. They should ship MCP + CLI, and add REST/Web only when they own state, workflows, dashboards, or other non-MCP consumers. Application/platform servers should expose direct product REST routes, not MCP protocol-shaped action envelopes.

## Endpoints

| Endpoint | Method | Auth | Purpose |
|---|---|---|---|
| `/health` | GET | Public | Fast liveness. Returns minimal status. |
| `/status` | GET | Public | Local-only redacted runtime status; see `docs/OBSERVABILITY.md`. |
| `/openapi.json` | GET | Public | Generated REST OpenAPI schema. |
| `/mcp` | POST/stream | Auth policy | Streamable HTTP MCP endpoint. |
| `/v1/capabilities` | GET | Auth policy | Route inventory and server metadata. |
| `/v1/greet` | POST | Auth policy | Direct `greet` action route. |
| `/v1/echo` | POST | Auth policy | Direct `echo` action route. |
| `/v1/status` | GET | Auth policy | Authenticated service-status action route. |
| `/v1/help` | GET | Auth policy | Action catalog and route help. |

## Direct REST requests

Preferred REST routes use ordinary product-shaped request bodies:

```json
{
  "message": "hello"
}
```

`GET` routes such as `/v1/status` and `/v1/help` do not require a body. REST does not expose an action-envelope route; action dispatch is reserved for the single MCP tool surface.

## REST handler

```rust
// src/api.rs
async fn v1_echo(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    match SomaAction::from_rest("echo", &body) {
        Ok(action) => run_rest_action(state, auth.as_ref().map(|Extension(auth)| auth), action).await,
        Err(error) => rest_error_response(error, "echo"),
    }
}
```

## Surface parity

| Surface | Call pattern |
|---|---|
| MCP | `soma(action="greet", name="Alice")` |
| REST | `POST /v1/greet {"name":"Alice"}` |
| CLI | `soma greet --name Alice` |

All three call `state.service.greet(Some("Alice"))`.

## Response shapes

```json
{"status":"ok"}
```

```json
{"echo":"hello"}
```

Responses are JSON values produced by `SomaService` via `crates/soma/contracts/src/actions.rs`.
If a REST result exceeds the response cap, the route returns a valid JSON
truncation envelope instead of raw truncated JSON.

## MCP-only actions

Some actions require MCP client capabilities and are excluded from REST action lists. Elicitation-based actions require a live MCP client interaction. REST `help` returns both `actions` and `mcp_only_actions` so clients can discover the split.

## Agent-first output rules

- No single response may return more than ~10,000 tokens (~40 KB). REST returns a JSON truncation envelope; MCP returns a valid structured page envelope with `_response_cursor` and `_response_offset` continuation arguments instead of invalid partial JSON; continuation calls read cached response data instead of re-running the original action.
- List actions MUST support `limit` and `offset` (or `cursor`).
- List actions that return heterogeneous data MUST support `filter` and `state` parameters.
- Every CLI command that outputs data MUST support `--json`.

```rust
const MAX_RESPONSE_BYTES: usize = 40_000; // ~10K tokens

fn mcp_response_page(serialized_bytes: usize, next_offset: usize) -> serde_json::Value {
    serde_json::json!({
        "kind": "mcp_response_page",
        "schema_version": 1,
        "code": "response_page",
        "message": "Tool response was returned as a scrollable serialized JSON page.",
        "truncated": false,
        "serialized_bytes": serialized_bytes,
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "content_format": "application/json-fragment",
        "content": "...serialized JSON page...",
        "page": {
            "offset": 0,
            "page_bytes": 16000,
            "next_offset": next_offset,
            "has_more": true
        },
        "continuation": {
            "tool": "soma",
            "arguments": {
                "_response_cursor": "rsp_...",
                "_response_offset": next_offset,
                "_response_page_bytes": 16000
            },
            "note": "Call the same tool with the same original arguments plus these reserved continuation arguments."
        }
    })
}
```

## Error messages

Errors must be actionable. Every error must say what failed, the bad value, why it failed, and how to fix it:

```rust
Ok(CallToolResult::structured_error(json!({
    "kind": "mcp_tool_error",
    "schema_version": 1,
    "code": "validation_error",
    "tool": "soma",
    "action": action,
    "message": reason,
    "retryable": true,
    "remediation": how_to_fix,
})))
```

Validation errors return HTTP 400 with an `error` field. Never leak secrets in error text.

Common error shapes:
- Missing required arg: `` "`id` is required for docker_logs — pass id=<container_id>" ``
- Unknown action: `"unknown action: \"florp\" — valid actions: greet, echo, status, help"`
- API unreachable: `"SOMA_URL unreachable: connection refused — is the service running?"`

MCP protocol auth/scope failures include structured `data` with `kind:
mcp_auth_error`, stable `code` values such as `missing_http_context`,
`missing_auth_context`, or `insufficient_scope`, and remediation text. MCP
execution failures are sanitized `mcp_tool_error` payloads with
`code: execution_error` plus a safe `reason_kind` such as `timeout`,
`rate_limited`, `auth_rejected`, `upstream_unavailable`, or `unknown`.

See `docs/PATTERNS.md` §A2, §39, §40 for the full REST pattern, error structure, and token discipline.
