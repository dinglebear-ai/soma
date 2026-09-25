use cortex_ingest_core::{metadata, normalize};
use serde_json::{Value, json};

#[test]
fn consumer_can_normalize_and_hash_without_cortex_runtime() {
    assert_eq!(normalize::NORMALIZER_VERSION, 1);

    let template =
        normalize::normalize_template("Failed password for alice from 10.0.0.1 port 2222 ssh2");
    assert_eq!(
        template,
        "Failed password for alice from <ip> port <n> ssh<n>"
    );

    let signature = normalize::signature_hash(&template);
    assert_eq!(signature.len(), 64);
    assert!(signature.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn consumer_can_bound_and_redact_metadata_without_storage() {
    let encoded = metadata::bounded_metadata_json(json!({
        "source_type": "consumer-test",
        "authorization": "Bearer secret",
        "nested": { "api_key": "also-secret" },
    }));
    let value: Value = serde_json::from_str(&encoded).expect("metadata should remain valid JSON");

    assert_eq!(value["source_type"], "consumer-test");
    assert_eq!(value["authorization"], "[REDACTED]");
    assert_eq!(value["nested"]["api_key"], "[REDACTED]");
}

#[test]
fn consumer_can_choose_lossless_bounded_metadata() {
    let small = json!({ "source_type": "consumer-test", "value": "ok" });
    assert!(metadata::try_bounded_metadata_json(small).is_some());

    let huge = json!({
        "source_type": "consumer-test",
        "values": vec![
            "x".repeat(metadata::MAX_METADATA_STRING_CHARS);
            metadata::MAX_METADATA_OBJECT_FIELDS
        ],
    });
    assert!(metadata::try_bounded_metadata_json(huge).is_none());
}
