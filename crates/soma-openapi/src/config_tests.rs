use crate::config::{OpenApiConfig, OpenApiCredential, OpenApiSpecConfig, SpecSource};

fn spec_with_secret_url() -> OpenApiSpecConfig {
    OpenApiSpecConfig {
        label: "vendor".into(),
        spec_source: SpecSource::Url(
            "https://user:pass@api.example.com/openapi.json?token=hunter2#secretfrag123"
                .parse()
                .unwrap(),
        ),
        base_url: "https://api.example.com".parse().unwrap(),
        allowed_operations: vec!["getUser".into()],
        credential: Some(OpenApiCredential::bearer_token("super-secret-token")),
    }
}

#[test]
fn debug_never_prints_credential_value() {
    let cfg = spec_with_secret_url();
    let dbg = format!("{cfg:?}");
    assert!(
        !dbg.contains("super-secret-token"),
        "credential leaked: {dbg}"
    );
    assert!(dbg.contains("vendor"));
}

#[test]
fn debug_never_prints_api_key_value() {
    let cred = OpenApiCredential::api_key("X-API-Key", "redaction-canary-api-key");
    let dbg = format!("{cred:?}");
    assert!(
        !dbg.contains("redaction-canary-api-key"),
        "api key leaked: {dbg}"
    );
    assert!(dbg.contains("X-API-Key"));
}

#[test]
fn debug_redacts_url_and_credentials() {
    let cfg = OpenApiConfig {
        specs: vec![spec_with_secret_url()],
    };
    let dbg = format!("{cfg:?}");
    for secret in [
        "hunter2",
        "user:pass",
        "pass@",
        "secretfrag123",
        "super-secret-token",
    ] {
        assert!(!dbg.contains(secret), "{secret} leaked: {dbg}");
    }
    assert!(dbg.contains("https://api.example.com/openapi.json"));
}

#[test]
fn credential_serialization_redacts_secret_values() {
    let cred = OpenApiCredential::api_key("X-Token", "secret-value");
    let json = serde_json::to_string(&cred).unwrap();
    assert!(json.contains("X-Token"));
    assert!(json.contains("<redacted>"));
    assert!(!json.contains("secret-value"));
}

#[test]
fn reserved_labels_are_known() {
    let mut spec = spec_with_secret_url();
    spec.label = "state".into();
    assert!(spec.is_reserved_label());
}
