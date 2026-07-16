use super::*;
use serde_json::json;

#[test]
fn sensitive_key_corpus_matches_secret_names() {
    for key in [
        "Authorization",
        "api-key",
        "client_secret",
        "refresh_token",
        "service_key",
        "password",
        "cookie",
    ] {
        assert!(is_sensitive_key(key), "{key}");
    }
    assert!(!is_sensitive_key("sort_key"));
}

#[test]
fn redaction_corpus_masks_urls_headers_oauth_and_split_flags() {
    let url = redact_url("https://user:pass@example.com/mcp?token=secret&page=1#frag");
    assert_eq!(url, "https://example.com/mcp?token=[redacted]&page=1");

    let args = redact_stdio_args(&[
        "--api-key".to_owned(),
        "secret".to_owned(),
        "--client-secret=top".to_owned(),
        "--mode".to_owned(),
        "safe".to_owned(),
    ]);
    assert_eq!(
        args,
        [
            "--api-key",
            "[redacted]",
            "--client-secret=[redacted]",
            "--mode",
            "safe"
        ]
    );

    let line = redact_log_line("Authorization: Bearer abc123 api_key=secret");
    assert!(!line.contains("abc123"));
    assert!(!line.contains("secret"));
}

#[test]
fn json_redaction_masks_nested_secret_values() {
    let raw = json!({
        "oauth": {"client_secret": "secret", "redirect": "https://example.com/cb"},
        "header": "Bearer abc123",
        "items": [{"token": "secret"}, {"name": "safe"}],
    });
    let rendered = redact_json_value(&raw).to_string();
    assert!(!rendered.contains(r#":"secret""#));
    assert!(!rendered.contains("abc123"));
    assert!(rendered.contains("safe"));
}
