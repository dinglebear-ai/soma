#![allow(dead_code)]

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::types::CodeModeExecutionResponse;

static SECRET_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:redaction-canary-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9_-]{20,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|glpat-[A-Za-z0-9_-]{20,}|xox[bp]-[A-Za-z0-9-]+|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)",
    )
    .expect("secret regex is valid")
});

pub fn redact_secret_like_segments(input: &str) -> String {
    let split_redacted = input
        .split_whitespace()
        .map(|segment| {
            if segment.starts_with("sk-")
                || segment.starts_with("redaction-canary-")
                || segment.starts_with("ghp_")
                || segment.starts_with("github_pat_")
                || segment.starts_with("glpat-")
                || segment.starts_with("xoxb-")
                || segment.starts_with("xoxp-")
                || segment.starts_with("eyJ")
            {
                "[REDACTED]".to_string()
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    SECRET_REGEX
        .replace_all(&split_redacted, "[REDACTED]")
        .into_owned()
}

pub(crate) fn sanitize_log_text(input: &str, max_len: usize) -> String {
    let mut value = input.to_string();
    value.retain(|ch| {
        !matches!(
            ch,
            '\u{0000}'..='\u{001F}'
                | '\u{007F}'..='\u{009F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
        )
    });
    for marker in ["<system>", "[INST]", "###", "<<"] {
        value = value.replace(marker, "");
    }
    redact_secret_like_segments(&value)
        .chars()
        .take(max_len)
        .collect()
}

pub(crate) fn truncate_execution_response(
    mut response: CodeModeExecutionResponse,
    max_response_bytes: usize,
    max_response_tokens: usize,
    token_estimate_divisor: u32,
) -> CodeModeExecutionResponse {
    if response_within_budget(
        &response,
        max_response_bytes,
        max_response_tokens,
        token_estimate_divisor,
    ) {
        return response;
    }
    if let Some(result) = response.result.take() {
        response.result = Some(truncation_marker(&result, token_estimate_divisor));
    }
    while !response.logs.is_empty()
        && !response_within_budget(
            &response,
            max_response_bytes,
            max_response_tokens,
            token_estimate_divisor,
        )
    {
        response.logs.remove(0);
    }
    response
}

pub(crate) fn response_within_budget(
    response: &CodeModeExecutionResponse,
    max_response_bytes: usize,
    max_response_tokens: usize,
    token_estimate_divisor: u32,
) -> bool {
    serde_json::to_vec(response).is_ok_and(|bytes| {
        bytes.len() <= max_response_bytes
            && estimated_tokens(bytes.len(), token_estimate_divisor) <= max_response_tokens.max(1)
    })
}

fn truncation_marker(value: &Value, divisor: u32) -> Value {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    json!({
        "truncated": true,
        "original_size": serialized.len(),
        "original_tokens": estimated_tokens(serialized.len(), divisor),
        "preview": crate::util::utf8_prefix_by_bytes(&serialized, 1024),
    })
}

fn estimated_tokens(byte_len: usize, divisor: u32) -> usize {
    byte_len.div_ceil(divisor.max(1) as usize).max(1)
}
