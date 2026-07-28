//! Diagnosis of malformed JSON payloads fetched in CI. Split out of
//! `rmcp_release_monitor.rs` to stay under the PATTERNS.md module size hard
//! limit.

use super::JSON_PREVIEW_BYTES;

/// Builds the `anyhow` context for a failed `serde_json` parse of a fetched
/// payload.
///
/// A bare serde message ("expected value at line 1 column 1") throws away the
/// only evidence that matters: what the fetch actually wrote. The scheduled
/// monitor failed opaquely for weeks because of it - the real payload was
/// ANSI-colorized JSON, which is indistinguishable from "empty file" or "API
/// error object" once the bytes are discarded. Report the size, a diagnosis
/// when the payload has a recognizable shape, and an escaped head.
pub(super) fn json_parse_context(label: &str, payload: &str) -> String {
    let mut message = format!("failed to parse {label}; {} bytes", payload.len());
    if let Some(diagnosis) = diagnose_json_payload(payload) {
        message.push_str("; ");
        message.push_str(diagnosis);
    }
    message.push_str("; payload head: ");
    message.push_str(&escaped_head(payload, JSON_PREVIEW_BYTES));
    message
}

/// Recognizes the payload shapes that actually show up when a JSON fetch goes
/// wrong in CI, and names the fix.
pub(super) fn diagnose_json_payload(payload: &str) -> Option<&'static str> {
    if payload.starts_with('\u{feff}') {
        return Some("payload starts with a UTF-8 byte-order mark");
    }
    let trimmed = payload.trim_start();
    if trimmed.is_empty() {
        return Some("payload is empty - the upstream fetch wrote nothing");
    }
    if trimmed.contains("\"message\"") && trimmed.contains("\"documentation_url\"") {
        return Some(
            "payload looks like a GitHub API error object - check the token, its permissions, \
             and the rate limit",
        );
    }
    match trimmed.chars().next()? {
        '\u{1b}' => Some(
            "payload starts with an ANSI escape - a CLI wrote colorized output instead of raw \
             JSON. `gh` forces color when CLICOLOR_FORCE is set to anything but 0, even with \
             stdout redirected to a file, and NO_COLOR does not override it; set \
             CLICOLOR_FORCE=0 on the fetch step",
        ),
        '<' => Some("payload starts with '<' - this looks like HTML, not JSON"),
        _ => None,
    }
}

/// Renders the first `max_bytes` of `payload` as a quoted, escaped, pure-ASCII
/// string so control characters (notably ESC) are visible in the CI log.
pub(super) fn escaped_head(payload: &str, max_bytes: usize) -> String {
    let mut end = payload.len().min(max_bytes);
    while !payload.is_char_boundary(end) {
        end -= 1;
    }
    let escaped: String = payload[..end]
        .chars()
        .flat_map(char::escape_debug)
        .collect();
    if end < payload.len() {
        format!("\"{escaped}\"...")
    } else {
        format!("\"{escaped}\"")
    }
}
