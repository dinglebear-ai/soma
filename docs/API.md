---
title: "API Reference"
doc_type: "guide"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "template"
source_of_truth: false
upstream_refs:
  - "docs/PATTERNS.md"
last_reviewed: "2026-05-15"
---

# API

The server exposes HTTP endpoints alongside MCP. All surfaces (MCP, REST, CLI) call the same `ExampleService` methods — no logic is duplicated.

## Endpoints

| Endpoint | Method | Auth | Purpose |
|---|---|---|---|
| `/health` | GET | Public | Fast liveness. Returns minimal status. |
| `/status` | GET | Public | Local-only redacted runtime status; see `docs/OBSERVABILITY.md`. |
| `/openapi.json` | GET | Public | Generated REST OpenAPI schema. |
| `/mcp` | POST/stream | Auth policy | Streamable HTTP MCP endpoint. |
| `/v1/example` | POST | Auth policy | REST action dispatch. |

## REST action request

The REST API uses the same `action` + `params` pattern as MCP tools:

```json
{
  "action": "echo",
  "params": {
    "message": "hello"
  }
}
```

`params` may be omitted or empty for no-argument actions.

## REST handler

```rust
// src/api.rs
async fn api_dispatch(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<ActionRequest>,
) -> impl IntoResponse {
    let result = match ExampleAction::from_rest(&body.action, &body.params) {
        Ok(action) => {
            if let Some(response) = enforce_rest_scope(
                &state,
                auth.as_ref().map(|Extension(auth)| auth),
                &body.action,
            ) {
                return response;
            }
            execute_service_action(&state.service, &action).await
        }
        Err(error) => Err(error),
    };

    match result {
        Ok(value) => Json(cap_rest_response(value)).into_response(),
        Err(e) if crate::actions::is_validation_error(&e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        ).into_response(),
        Err(e) => {
            tracing::error!(error = %e, action = %body.action, "REST action execution failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            ).into_response()
        }
    }
}
```

## Surface parity

| Surface | Call pattern |
|---|---|
| MCP | `example(action="greet", name="Alice")` |
| REST | `POST /v1/example {"action":"greet","params":{"name":"Alice"}}` |
| CLI | `example greet --name Alice` |

All three call `state.service.greet(Some("Alice"))`.

## Response shapes

```json
{"status":"ok"}
```

```json
{"echo":"hello"}
```

Responses are JSON values produced by `ExampleService` via `src/actions.rs`.
If a REST action result exceeds the response cap, the route returns a valid JSON
truncation envelope instead of raw truncated JSON.

## MCP-only actions

Some actions require MCP client capabilities and are excluded from REST action lists. Elicitation-based actions require a live MCP client interaction. REST `help` returns both `actions` and `mcp_only_actions` so clients can discover the split.

## Agent-first output rules

- No single response may return more than ~10,000 tokens (~40 KB). REST returns a JSON truncation envelope; MCP returns a valid structured page envelope with `_response_offset` continuation arguments instead of invalid partial JSON.
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
        "truncated": false,
        "serialized_bytes": serialized_bytes,
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "content_format": "application/json-fragment",
        "content": "...serialized JSON page...",
        "continuation": {
            "arguments": {
                "_response_offset": next_offset,
                "_response_page_bytes": 16000
            }
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
    "tool": "example",
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
- API unreachable: `"EXAMPLE_URL unreachable: connection refused — is the service running?"`

MCP protocol auth/scope failures include structured `data` with `kind:
mcp_auth_error`, stable `code` values such as `missing_http_context`,
`missing_auth_context`, or `insufficient_scope`, and remediation text. MCP
execution failures are sanitized `mcp_tool_error` payloads with
`code: execution_error` plus a safe `reason_kind` such as `timeout`,
`rate_limited`, `auth_rejected`, `upstream_unavailable`, or `unknown`.

See `docs/PATTERNS.md` §A2, §39, §40 for the full REST pattern, error structure, and token discipline.
