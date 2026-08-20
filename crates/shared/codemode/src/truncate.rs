#![allow(dead_code)]

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

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

    // Only replace the final result when the marker actually shrinks it. A
    // logs-dominant response can otherwise turn a tiny valid result into a
    // larger marker and make the envelope harder to fit.
    if let Some(result) = response.result.as_ref() {
        let original_len = serde_json::to_string(result).map_or(0, |value| value.len());
        let marker = truncation_marker(
            result,
            token_estimate_divisor,
            marker_byte_budget(
                &response,
                max_response_bytes,
                max_response_tokens,
                token_estimate_divisor,
            ),
        );
        let marker_len = serde_json::to_string(&marker).map_or(usize::MAX, |value| value.len());
        if marker_len < original_len {
            response.result = Some(marker);
        }
    }

    if !response.logs.is_empty()
        && !response_within_budget(
            &response,
            max_response_bytes,
            max_response_tokens,
            token_estimate_divisor,
        )
    {
        trim_logs_to_budget(
            &mut response,
            max_response_bytes,
            max_response_tokens,
            token_estimate_divisor,
        );
    }

    response
}

fn trim_logs_to_budget(
    response: &mut CodeModeExecutionResponse,
    max_response_bytes: usize,
    max_response_tokens: usize,
    token_estimate_divisor: u32,
) {
    let original = std::mem::take(&mut response.logs);
    let total = original.len();
    let Ok(base) = serde_json::to_vec(&*response) else {
        return;
    };
    let base_len = base.len();
    let base_is_empty_object = base == b"{}";
    let fits = |len: usize| {
        len <= max_response_bytes
            && estimated_tokens(len, token_estimate_divisor) <= max_response_tokens.max(1)
    };

    // If the non-log response is already over budget, logs cannot repair it.
    // Keep them dropped rather than making the overflow worse.
    if !fits(base_len) {
        return;
    }

    let line_lens = original
        .iter()
        .map(|line| serde_json::to_string(line).map_or(line.len() + 2, |value| value.len()))
        .collect::<Vec<_>>();
    let mut kept_sum = line_lens.iter().copied().sum::<usize>();

    for drop_count in 1..=total {
        kept_sum = kept_sum.saturating_sub(line_lens[drop_count - 1]);
        let kept = total - drop_count;
        let sentinel =
            format!("[logs truncated to fit response budget: {drop_count} line(s) dropped]");
        let sentinel_len =
            serde_json::to_string(&sentinel).map_or(sentinel.len() + 2, |value| value.len());
        let item_count = kept + 1;
        let logs_field_overhead = if base_is_empty_object { 9 } else { 10 };
        let candidate_len = base_len
            .saturating_add(logs_field_overhead)
            .saturating_add(sentinel_len)
            .saturating_add(kept_sum)
            .saturating_add(item_count.saturating_sub(1));
        if fits(candidate_len) {
            let mut candidate = Vec::with_capacity(item_count);
            candidate.push(sentinel);
            candidate.extend_from_slice(&original[drop_count..]);
            response.logs = candidate;
            return;
        }
    }
}

pub(crate) fn response_within_budget(
    response: &CodeModeExecutionResponse,
    max_response_bytes: usize,
    max_response_tokens: usize,
    token_estimate_divisor: u32,
) -> bool {
    match serde_json::to_vec(response) {
        Ok(bytes) => {
            bytes.len() <= max_response_bytes
                && estimated_tokens(bytes.len(), token_estimate_divisor)
                    <= max_response_tokens.max(1)
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to serialize Code Mode response while enforcing the response budget"
            );
            false
        }
    }
}

fn marker_byte_budget(
    response: &CodeModeExecutionResponse,
    max_response_bytes: usize,
    max_response_tokens: usize,
    divisor: u32,
) -> usize {
    // Size the marker against the exact non-result envelope that remains after
    // log trimming. This avoids both an arbitrary safety margin and a marker
    // that fits in isolation but overflows once calls/error/UI metadata are added.
    let mut base = response.clone();
    base.result = None;
    base.logs.clear();
    let base = match serde_json::to_vec(&base) {
        Ok(base) => base,
        Err(_) => return 0,
    };
    let effective_budget = max_response_bytes.min(
        max_response_tokens
            .max(1)
            .saturating_mul(divisor.max(1) as usize),
    );
    // Adding a result to an empty object costs 9 bytes beyond the existing braces;
    // adding it to a non-empty object costs 10 bytes for the comma and field name.
    let result_field_overhead = if base == b"{}" { 9 } else { 10 };
    effective_budget
        .saturating_sub(base.len())
        .saturating_sub(result_field_overhead)
}

fn truncation_marker(value: &Value, divisor: u32, max_bytes: usize) -> Value {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let original_size = serialized.len();
    let original_tokens = estimated_tokens(original_size, divisor);
    let full_marker = |preview: &str| {
        json!({
            "truncated": true,
            "original_size": original_size,
            "original_tokens": original_tokens,
            "preview": preview,
            "next_action": "Use a narrower query, request fewer fields, or split the work across multiple codemode calls."
        })
    };

    if crate::util::serialized_size(&full_marker("")) <= max_bytes {
        let mut low = 0usize;
        let mut high = original_size.min(1024);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            let preview = crate::util::utf8_prefix_by_bytes(&serialized, mid);
            if crate::util::serialized_size(&full_marker(preview)) <= max_bytes {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        return full_marker(crate::util::utf8_prefix_by_bytes(&serialized, low));
    }

    let compact = json!({
        "truncated": true,
        "original_size": original_size,
        "original_tokens": original_tokens,
    });
    if crate::util::serialized_size(&compact) <= max_bytes {
        compact
    } else {
        json!({"truncated": true})
    }
}

fn estimated_tokens(byte_len: usize, divisor: u32) -> usize {
    byte_len.div_ceil(divisor.max(1) as usize).max(1)
}
