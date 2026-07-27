use serde_json::{json, Value};
use soma_provider_core::{ProviderCall, ProviderSurface};

use super::{
    decode_python_response, encode_python_request, validate_python_response, PythonProtocolError,
    PythonWorkerRequest, PythonWorkerResponse, ONE_SHOT_REQUEST_ID, PYTHON_WORKER_SCHEMA_VERSION,
};

#[test]
fn catalog_request_has_stable_versioned_shape() {
    let request = PythonWorkerRequest::catalog(std::path::Path::new("/tmp/provider.py"));
    let encoded = encode_python_request(&request).expect("catalog request should encode");

    assert_eq!(encoded.last(), Some(&b'\n'));
    assert_eq!(
        encoded.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "one request must produce exactly one NDJSON frame"
    );
    let value: Value = serde_json::from_slice(&encoded).expect("request JSON");
    assert_eq!(
        value,
        json!({
            "mode": "catalog",
            "schema_version": PYTHON_WORKER_SCHEMA_VERSION,
            "request_id": ONE_SHOT_REQUEST_ID,
            "path": "/tmp/provider.py"
        })
    );
}

#[test]
fn call_request_preserves_execution_envelope_fields() {
    let mut call = ProviderCall::new(
        "lookup",
        json!({"query": "first
second"}),
    )
    .with_surface(ProviderSurface::Cli);
    call.provider = "demo-python".to_owned();
    call.snapshot_id = "sha256:test-snapshot".to_owned();

    let request = PythonWorkerRequest::call(
        std::path::Path::new("/tmp/demo.py"),
        &call,
        vec!["SOMA_DEMO_SECRET".to_owned()],
    );
    let encoded = encode_python_request(&request).expect("call request should encode");

    assert_eq!(encoded.last(), Some(&b'\n'));
    assert_eq!(
        encoded.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "newlines inside JSON strings must remain escaped"
    );
    let value: Value = serde_json::from_slice(&encoded).expect("request JSON");
    assert_eq!(value["mode"], "call");
    assert_eq!(value["schema_version"], PYTHON_WORKER_SCHEMA_VERSION);
    assert_eq!(value["request_id"], ONE_SHOT_REQUEST_ID);
    assert_eq!(value["path"], "/tmp/demo.py");
    assert_eq!(value["env_keys"], json!(["SOMA_DEMO_SECRET"]));
    assert_eq!(value["provider"], "demo-python");
    assert_eq!(value["action"], "lookup");
    assert_eq!(
        value["params"],
        json!({"query": "first
second"})
    );
    assert_eq!(value["surface"], "cli");
    assert_eq!(value["snapshot_id"], "sha256:test-snapshot");
}

#[test]
fn responses_preserve_raw_catalogs_and_call_outputs() {
    let catalog_request = PythonWorkerRequest::catalog(std::path::Path::new("demo.py"));
    let raw_catalog = json!({
        "schema_version": 1,
        "provider": {"name": "demo", "kind": "python"},
        "tools": [],
        "unknown_until_schema_validation": {"kept": true}
    });
    let catalog_bytes = serde_json::to_vec(&PythonWorkerResponse::Catalog {
        schema_version: PYTHON_WORKER_SCHEMA_VERSION,
        request_id: ONE_SHOT_REQUEST_ID,
        catalog: raw_catalog.clone(),
    })
    .expect("catalog response JSON");
    let catalog_response =
        decode_python_response(&catalog_bytes).expect("catalog response should decode");
    validate_python_response(&catalog_request, &catalog_response)
        .expect("catalog response should match request");
    assert!(matches!(
        catalog_response,
        PythonWorkerResponse::Catalog { catalog, .. } if catalog == raw_catalog
    ));

    let call = ProviderCall::new("lookup", json!({}));
    let call_request = PythonWorkerRequest::call(std::path::Path::new("demo.py"), &call, vec![]);
    let output = json!({"items": [1, 2, 3], "ok": true});
    let call_bytes = serde_json::to_vec(&PythonWorkerResponse::Call {
        schema_version: PYTHON_WORKER_SCHEMA_VERSION,
        request_id: ONE_SHOT_REQUEST_ID,
        output: output.clone(),
    })
    .expect("call response JSON");
    let call_response = decode_python_response(&call_bytes).expect("call response should decode");
    validate_python_response(&call_request, &call_response)
        .expect("call response should match request");
    assert!(matches!(
        call_response,
        PythonWorkerResponse::Call { output: value, .. } if value == output
    ));
}

#[test]
fn protocol_rejects_version_id_and_mode_mismatches() {
    let wrong_version = br#"{"mode":"call","schema_version":2,"request_id":0,"output":null}"#;
    assert!(matches!(
        decode_python_response(wrong_version),
        Err(PythonProtocolError::UnsupportedSchemaVersion {
            expected: PYTHON_WORKER_SCHEMA_VERSION,
            actual: 2
        })
    ));

    let request = PythonWorkerRequest::catalog(std::path::Path::new("demo.py"));
    let wrong_id = PythonWorkerResponse::Catalog {
        schema_version: PYTHON_WORKER_SCHEMA_VERSION,
        request_id: 7,
        catalog: json!({}),
    };
    assert!(matches!(
        validate_python_response(&request, &wrong_id),
        Err(PythonProtocolError::UnexpectedRequestId {
            expected: ONE_SHOT_REQUEST_ID,
            actual: 7
        })
    ));

    let wrong_mode = PythonWorkerResponse::Call {
        schema_version: PYTHON_WORKER_SCHEMA_VERSION,
        request_id: ONE_SHOT_REQUEST_ID,
        output: json!(null),
    };
    assert!(matches!(
        validate_python_response(&request, &wrong_mode),
        Err(PythonProtocolError::UnexpectedResponseMode {
            expected: "catalog",
            actual: "call"
        })
    ));
}
