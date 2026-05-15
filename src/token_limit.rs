//! Response size cap — prevents context-window exhaustion in MCP clients.
//!
//! # TEMPLATE: The 10K token philosophy
//!
//! MCP servers communicate with AI agents that have finite context windows.
//! A single oversized response can consume a large fraction of that window,
//! leaving little room for the agent's reasoning and subsequent tool calls.
//!
//! **Rule**: no single MCP tool response may exceed ~10,000 tokens (~40KB).
//!
//! ## Why 40KB?
//!
//! - ~4 bytes/token on average (English prose, JSON, code)
//! - 40,000 bytes / 4 bytes ≈ 10,000 tokens
//! - 10K tokens is a generous upper bound that fits comfortably in any modern
//!   LLM context window without crowding out reasoning
//!
//! ## What to do instead of returning huge responses
//!
//! 1. **Paginate**: add `limit`/`offset` parameters to list actions
//! 2. **Filter**: add `filter` or `query` parameters to narrow results
//! 3. **Summarize**: return counts and top-N items, with a link to get more
//! 4. **Stream**: for logs/events, return the most recent N lines
//!
//! ## Where to apply truncation
//!
//! Apply `truncate_if_needed()` in `mcp/tools.rs` AFTER the service call,
//! BEFORE constructing the `CallToolResult`. Example:
//!
//! ```rust,ignore
//! use rmcp_template::token_limit;
//!
//! let result = state.service.list_things(limit, offset).await?;
//! let text = serde_json::to_string_pretty(&result)?;
//! let text = token_limit::truncate_if_needed(&text);
//! Ok(json!({"result": text}))
//! ```
//!
//! Or for the whole serialized response:
//!
//! ```rust,ignore
//! let json = serde_json::to_string(&result)?;
//! let json = token_limit::truncate_if_needed(&json);
//! ```
//!
//! ## Truncation is a safety net, not the primary strategy
//!
//! Truncation is the last resort. Design your actions to return bounded data
//! by default (limit=50, summary-only, etc.) so truncation rarely triggers.
//! When it does trigger, the truncation message tells the agent exactly what
//! to do next.

/// Maximum response size in bytes.
///
/// This constant is the single source of truth for the 10K token cap.
/// Change it here to adjust the cap for all actions simultaneously.
///
/// # TEMPLATE: Adjusting the cap
///
/// For services that return very dense data (e.g. binary-encoded metrics),
/// you may want a lower cap. For services that return sparse text (e.g.
/// configuration files), the cap may be relaxed slightly.
///
/// Never exceed 100KB (25K tokens) — at that size, agents start losing
/// context from earlier in the conversation.
pub const MAX_RESPONSE_BYTES: usize = 40_000;

/// Truncate `text` to [`MAX_RESPONSE_BYTES`] if it exceeds the cap.
///
/// When truncation occurs, appends a clear notice telling the agent:
/// 1. That the response was truncated (not an error)
/// 2. The exact token limit that was hit
/// 3. How to get the full data (use pagination/filters)
///
/// # Truncation boundary
///
/// Truncation finds the last valid UTF-8 boundary within the content budget.
/// The returned string, including the notice, never exceeds
/// [`MAX_RESPONSE_BYTES`].
///
/// # TEMPLATE: Returning the raw truncated string
///
/// This function returns a `String`, not a `Value`. The caller wraps it
/// as appropriate:
///
/// ```rust,ignore
/// // In tools.rs:
/// let raw = serde_json::to_string(&result)?;
/// let output = token_limit::truncate_if_needed(&raw);
/// // output is now a plain string — wrap it for the tool result:
/// Ok(json!({ "data": output }))
/// ```
///
/// Or embed the truncation check inside the serialized JSON directly:
///
/// ```rust,ignore
/// let text = serde_json::to_string_pretty(&result)?;
/// let text = token_limit::truncate_if_needed(&text);
/// tool_text_result(text)  // helper that wraps in CallToolResult
/// ```
#[must_use]
pub fn truncate_if_needed(text: &str) -> String {
    if text.len() <= MAX_RESPONSE_BYTES {
        return text.to_string();
    }

    let notice = format!(
        "\n\n[TRUNCATED: response exceeded {MAX_RESPONSE_BYTES} bytes (~10K tokens).\n\
        Use limit/offset parameters or more specific filters to get a smaller result.\n\
        Example: action=things, limit=20, offset=0]"
    );
    let content_budget = MAX_RESPONSE_BYTES.saturating_sub(notice.len());

    // Find the last valid UTF-8 boundary within the content budget so the final
    // string including the notice does not exceed MAX_RESPONSE_BYTES.
    // `str::floor_char_boundary` is stable since Rust 1.86 (MSRV is 1.90).
    let boundary = text.floor_char_boundary(content_budget);
    let truncated = &text[..boundary];

    format!("{truncated}{notice}")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_passes_through_unchanged() {
        let text = "hello world";
        assert_eq!(truncate_if_needed(text), text);
    }

    #[test]
    fn empty_string_passes_through() {
        assert_eq!(truncate_if_needed(""), "");
    }

    #[test]
    fn text_at_exact_limit_passes_through() {
        let text = "x".repeat(MAX_RESPONSE_BYTES);
        let result = truncate_if_needed(&text);
        assert!(!result.contains("[TRUNCATED"));
        assert_eq!(result.len(), MAX_RESPONSE_BYTES);
    }

    #[test]
    fn text_over_limit_is_truncated() {
        let text = "x".repeat(MAX_RESPONSE_BYTES + 100);
        let result = truncate_if_needed(&text);
        assert!(result.contains("[TRUNCATED"));
        assert!(result.contains("limit/offset"));
        assert!(result.len() <= MAX_RESPONSE_BYTES);
    }

    #[test]
    fn truncation_notice_mentions_token_limit() {
        let text = "y".repeat(MAX_RESPONSE_BYTES + 1);
        let result = truncate_if_needed(&text);
        assert!(result.contains("10K tokens"));
    }

    #[test]
    fn truncates_at_utf8_boundary() {
        // Build a string where the truncation budget falls inside a multi-byte char.
        // Each '€' is 3 bytes (0xE2 0x82 0xAC). Fill just past the limit.
        let mut text = "a".repeat(MAX_RESPONSE_BYTES - 1);
        text.push('€'); // starts at byte MAX_RESPONSE_BYTES-1, ends at MAX_RESPONSE_BYTES+1
        let result = truncate_if_needed(&text);
        let notice_start = result.find("[TRUNCATED").expect("notice should be present") - 2;
        assert!(result[..notice_start].chars().all(|c| c == 'a'));
        assert!(result.len() <= MAX_RESPONSE_BYTES);
        assert!(result.contains("[TRUNCATED"));
    }
}
