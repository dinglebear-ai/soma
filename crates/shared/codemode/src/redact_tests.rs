use super::redact::{is_sensitive_key, redact_stdio_args, redact_url};

#[test]
fn redacts_sensitive_keys_and_urls() {
    assert!(is_sensitive_key("terminal_id"));
    assert_eq!(
        redact_url("https://user:pass@example.com/path?token=secret&ok=1#frag"),
        "https://example.com/path?token=[redacted]&ok=1"
    );
}

#[test]
fn redacts_stdio_flags() {
    let args = vec![
        "--api-key".to_string(),
        "secret".to_string(),
        "--plain=value".to_string(),
    ];
    assert_eq!(
        redact_stdio_args(&args),
        vec![
            "--api-key".to_string(),
            "[redacted]".to_string(),
            "--plain=value".to_string()
        ]
    );
}
