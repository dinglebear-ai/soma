---
title: "MCP Schema Contract"
created: 2026-05-14
updated: 2026-07-30
---

# MCP Schema Contract

Generated from `crates/soma/domain/src/actions.rs` and checked against the schema, README, skill docs, help text, and scope routing.

Run:

```bash
cargo xtask check-schema-docs --write
cargo xtask check-schema-docs --check
```

## Tool

| Field | Value |
|---|---|
| Tool name | `soma` |
| Schema resource | `soma://schema/mcp-tool` |
| Dispatch parameter | `action` |

## Actions

| Action | Scope | Cost | Description |
|---|---|---|---|
| `greet` | `soma:read` | `cheap` | Return a greeting. |
| `echo` | `soma:read` | `cheap` | Echo a message back unchanged. |
| `status` | `soma:read` | `cheap` | Return server status and configuration info. |
| `python_environment_status` | `soma:write` | `cheap` | Inspect immutable Python environment cache state without executing provider code. |
| `python_environment_prune_plan` | `soma:write` | `moderate` | Plan a bounded prune of stale non-ready Python environment cache entries. |
| `python_environment_prune` | `soma:write` | `write` | Apply a bounded prune of stale non-ready Python environment cache entries. |
| `python_environment_repair` | `soma:write` | `write` | Repair the immutable environment for one managed Python provider. |
| `python_environment_update` | `soma:write` | `write` | Resolve, prepare, validate, and atomically activate an immutable update for one managed Python provider. |
| `python_worker_status` | `soma:write` | `cheap` | Inspect persistent Python worker health, quarantine, restart counts, and bounded redacted logs. |
| `python_worker_cancel` | `soma:write` | `write` | Cancel one active persistent Python invocation by terminating its process tree. |
| `python_worker_reset` | `soma:write` | `write` | Clear one persistent Python worker crash-loop quarantine. |
| `python_generation_status` | `soma:read` | `cheap` | Inspect the active Python provider generation and bounded rollback history. |
| `python_generation_rollback` | `soma:write` | `write` | Atomically reactivate a retained Python provider generation. |
| `elicit_name` | `soma:read` | `cheap` | Ask the MCP client to collect a name, then return a personalised greeting. |
| `scaffold_intent` | `soma:read` | `moderate` | Collect scaffold setup intent through MCP elicitation and return JSON for the scaffold-project skill. |
| `help` | public | `cheap` | Show the action reference. |

## Drift Rules

- `ACTION_SPECS` in `crates/soma/domain/src/actions.rs` is the canonical action and scope list.
- Action cost is planner metadata. Use `cheap` for first-pass reads, `moderate` for bounded workflow setup, `expensive` for broad scans or long-running work, and `write` for mutating operations.
- `crates/soma/mcp/src/schemas.rs` must derive its enum from `ACTION_SPECS`.
- The MCP tool schema must reject unknown top-level parameters except reserved `_response_*` continuation fields, and encode action-specific requirements that fit the single-tool dispatch model.
- `help` is intentionally public and must have no required scope.
- `crates/soma/mcp/src/tools.rs`, `README.md`, and `plugins/soma/skills/soma/SKILL.md` must mention every action.
- `crates/soma/mcp/src/rmcp_server.rs` owns stable resources and must keep `soma://schema/mcp-tool` wired to `tool_definitions()`.
- `crates/soma/mcp/src/prompts.rs` owns stable prompts and must keep `quick_start` covered by prompt tests.

## Resources

| URI | Source | Contract |
|---|---|---|
| `soma://schema/mcp-tool` | `crates/soma/mcp/src/rmcp_server.rs` | Returns `tool_definitions()` as `application/json`. |

## Prompts

| Prompt | Source | Contract |
|---|---|---|
| `quick_start` | `crates/soma/mcp/src/prompts.rs` | Guides a client to call `status` and `greet`. |

## Input Validation

- `action` is always required.
- `echo` conditionally requires non-empty `message`.
- `greet` accepts optional `name` and defaults to World.
- `elicit_name` and `scaffold_intent` collect their extra fields through MCP elicitation, not direct tool-call arguments.
- Unknown top-level parameters are rejected by the schema except reserved MCP adapter continuation fields.

## Reserved Adapter Parameters

Oversized MCP responses are returned as `kind=mcp_response_page` envelopes. Continuation calls reuse the same tool and original arguments, plus these reserved fields:

| Parameter | Type | Purpose |
|---|---|---|
| `_response_cursor` | string | Cursor for cached serialized response data. Required with `_response_offset`. |
| `_response_offset` | integer | Byte offset into the cached serialized response. |
| `_response_page_bytes` | integer | Page size in bytes, from 1 to 16000. |

The adapter strips these fields before dispatching to the service layer.
