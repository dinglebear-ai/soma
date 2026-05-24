# Agent-Helpful MCP Errors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make tool-originated MCP failures visible, structured, and recoverable for agents.

**Architecture:** Keep true protocol failures as `ErrorData`: auth/scope denial, unknown MCP tool names, resource/prompt lookup failures, and serialization/server defects. Convert action validation and action execution failures into `CallToolResult::structured_error` payloads so the model can inspect `isError`, `code`, `field`, `action`, and remediation text.

**Tech Stack:** Rust, rmcp 1.7 `CallToolResult::structured_error`, serde_json, existing `anyhow` service boundary.

---

### File Structure

- Modify `src/mcp/rmcp_server.rs`: add structured tool-error helpers; convert validation/runtime action failures; keep unknown tool/auth/resource/prompt errors as protocol errors.
- Modify `src/actions.rs`: expose structured metadata for `ValidationError`.
- Modify `src/app.rs`: expose structured metadata for `ScaffoldIntentValidationError`.
- Modify `src/mcp/rmcp_server_tests.rs`: cover structured error helper behavior and unknown-action conversion.
- Modify `tests/tool_dispatch.rs`: preserve low-level dispatcher tests but add assertions that helper-level errors are structured where applicable.
- Modify docs: `docs/PATTERNS.md`, `docs/TESTING.md`, `README.md`, and `CLAUDE.md` so template guidance matches the implementation.

### Task 1: Structured Error Metadata

**Files:**
- Modify: `src/actions.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Add failing metadata tests**

Add tests that assert `ValidationError::MissingField { field: "message" }` exposes `code=missing_field`, `field=message`, and a remediation that names `action=help`.

- [ ] **Step 2: Implement metadata methods**

Add methods like:

```rust
impl ValidationError {
    pub fn code(&self) -> &'static str { /* missing_action, missing_field, wrong_type, not_available_over_rest, unknown_action */ }
    pub fn field(&self) -> Option<&str> { /* action or field when known */ }
    pub fn remediation(&self) -> String { /* concrete retry guidance */ }
}
```

For `ScaffoldIntentValidationError`, add `code`, `field`, and `remediation` fields so invalid URL/identifier/env/port failures can name the field and fix.

### Task 2: MCP Tool Error Results

**Files:**
- Modify: `src/mcp/rmcp_server.rs`

- [ ] **Step 1: Add helper tests**

Add tests that build tool errors and assert:

```rust
assert_eq!(result.is_error, Some(true));
assert_eq!(result.structured_content.as_ref().unwrap()["kind"], "mcp_tool_error");
assert_eq!(result.structured_content.as_ref().unwrap()["schema_version"], 1);
```

- [ ] **Step 2: Add helper functions**

Implement helpers:

```rust
fn tool_error_result(payload: Value) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string(&payload)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let text = token_limit::truncate_if_needed(&text);
    let mut result = CallToolResult::structured_error(payload);
    result.content = vec![Content::text(text)];
    Ok(result)
}
```

Use payloads with `kind`, `schema_version`, `code`, `tool`, `action`, `message`, `retryable`, `remediation`, and optional `field`, `bad_value`, `available_actions`, `available_tools`.

- [ ] **Step 3: Convert action failures**

In `call_tool`, convert known-tool validation failures and action execution failures to `tool_error_result(...)`. Keep `require_auth_context`, `check_scope`, unknown MCP tool name, resource errors, prompt errors, and serialization failures as protocol errors.

### Task 3: Tests And Docs

**Files:**
- Modify: `src/mcp/rmcp_server_tests.rs`
- Modify: `tests/tool_dispatch.rs`
- Modify: `docs/PATTERNS.md`
- Modify: `docs/TESTING.md`
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update tests**

Add or revise tests for missing action, unknown action, missing echo message, wrong echo type, and scaffold validation. Each should verify `is_error == Some(true)` and structured content with concrete `code`, `field`, and remediation.

- [ ] **Step 2: Align docs**

Remove the contradictory `Err(anyhow!(...))` “good” MCP example and replace it with `CallToolResult::structured_error`. Add testing guidance that negative MCP cases must assert `isError` and structured payload fields, not only substring messages.

- [ ] **Step 3: Verify**

Run:

```bash
cargo fmt
cargo test
cargo clippy -- -D warnings
```

Expected: all pass.
