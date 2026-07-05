---
title: "Agents-First Design"
doc_type: "guide"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "template"
source_of_truth: false
last_reviewed: "2026-05-15"
---

# Agents-first design

This template is optimized for AI agents as primary operators and consumers. Design rule: if an agent can't use it cleanly, fix the output, not the agent.

## Design rules

- Return stable JSON objects with predictable keys.
- Keep responses compact; cap large outputs and summarize by default.
- Include actionable error messages with remediation hints.
- Make all actions discoverable through `action="help"` and `docs/MCP_SCHEMA.md`.
- Prefer semantic test assertions so agents can trust examples.

## Token discipline

No single response may return more than ~10,000 tokens (~40 KB of text). MCP
responses must stay valid JSON; when a serialized tool result is too large,
return a small structured page envelope with `_response_cursor` and
`_response_offset` continuation arguments. List actions should still be
paginated by default. Continuation calls use the cursor to read cached response
data instead of re-running the original action:

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
            "tool": "example",
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

List actions MUST support `limit` and `offset`. Response shape includes pagination metadata:

```json
{
  "items": [...],
  "total": 1842,
  "limit": 50,
  "offset": 0,
  "has_more": true,
  "next_offset": 50
}
```

## Informative errors

Every error must answer four questions:

| Field | Example |
|---|---|
| What failed | `"echo: message is required"` |
| The bad value | `"id=\"abc123\""` |
| Why it failed | `"container may be stopped or removed"` |
| How to fix | `"use action=help to see required parameters"` |

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

Use MCP protocol errors only for protocol/security failures such as unknown tool
names, auth/scope denial, resource lookup, prompt lookup, and serialization
defects. Action validation and action execution failures should be visible tool
results with `isError: true`.

Never return opaque `"internal error"` messages. Never leak secrets in error text.

## Transport surfaces

Agents may use:

1. **MCP tool calls** through `/mcp` or stdio (preferred — full tool schema, scope enforcement)
2. **CLI commands** for local shell workflows (`example greet --name Alice`)
3. **Direct REST routes** when MCP tooling is unavailable (`POST /v1/greet {"name":"Alice"}`)
4. **Plugin skills** as human/agent guidance

The action metadata in `crates/rtemplate-service/src/actions.rs` keeps these surfaces aligned. Every non-MCP-only action that the MCP tool exposes must also be reachable from the CLI and direct REST routes when its transport metadata allows those surfaces.

## Summarize by default, expand on request

```
# Default: summary view (fits on screen)
$ example things
  ID   NAME               STATE    UPDATED
  42   my-thing           active   2m ago
  43   other-thing        idle     1h ago

# Full detail: --verbose or specific action
$ example thing 42
$ example thing 42 --json
```

## Documentation contract

When adding an action, update:

- `crates/rtemplate-service/src/actions.rs` for metadata, validation, and native dispatch
- `crates/rtemplate-service/src/app.rs` for business behavior
- Generated MCP schema docs and OpenAPI after the registry changes
- `crates/rmcp-template/tests/tool_dispatch.rs`, CLI tests, and REST route tests
- `docs/MCP_SCHEMA.md`
- Plugin skill docs

## Security for agents

Never place secrets in skill text, generated docs, or examples. Sensitive plugin settings must be marked `sensitive: true` and passed through environment variables or headers.

See `docs/PATTERNS.md` §39 and §40 for the full error message and token discipline patterns.
