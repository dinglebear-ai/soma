use serde_json::{json, Value};
use soma_provider_core::{ProviderCall, ProviderSurface};

use super::{
    decode_python_response, decode_runner_frame, encode_python_request, encode_runner_frame,
    negotiate_runner_features, validate_python_response, PythonActorContext,
    PythonInvocationRequest, PythonInvocationState, PythonProtocolError, PythonRunnerError,
    PythonRunnerErrorCode, PythonRunnerErrorPhase, PythonRunnerFeature, PythonRunnerHello,
    PythonRunnerHostCall, PythonRunnerHostRequest, PythonRunnerProtocolVersion, PythonRunnerReply,
    PythonRuntimeIdentity, PythonTraceContext, PythonWorkerHealth, PythonWorkerRequest,
    PythonWorkerResponse, ONE_SHOT_REQUEST_ID, PYTHON_RUNNER_MAX_FRAME_BYTES,
    PYTHON_WORKER_SCHEMA_VERSION,
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

#[test]
fn runner_handshake_negotiates_minor_version_and_features() {
    let hello = PythonRunnerHello {
        protocol: PythonRunnerProtocolVersion { major: 1, minor: 4 },
        sdk_version: "0.1.0".to_owned(),
        python: PythonRuntimeIdentity {
            implementation: "cpython".to_owned(),
            version: "3.13.5".to_owned(),
        },
        features: vec![
            PythonRunnerFeature::Describe,
            PythonRunnerFeature::Invoke,
            PythonRunnerFeature::Health,
        ],
    };

    let frame = encode_runner_frame(&hello).expect("hello should frame");
    let declared = u32::from_be_bytes(frame[..4].try_into().expect("length prefix")) as usize;
    assert_eq!(declared, frame.len() - 4);
    let decoded: PythonRunnerHello = decode_runner_frame(&frame).expect("hello should decode");
    assert_eq!(decoded, hello);

    assert_eq!(
        PythonRunnerProtocolVersion::current()
            .negotiate(hello.protocol)
            .expect("matching majors should negotiate"),
        PythonRunnerProtocolVersion { major: 1, minor: 0 }
    );
    assert!(matches!(
        PythonRunnerProtocolVersion::current()
            .negotiate(PythonRunnerProtocolVersion { major: 2, minor: 0 }),
        Err(PythonProtocolError::ProtocolMajorMismatch { host: 1, worker: 2 })
    ));

    assert_eq!(
        negotiate_runner_features(
            &[
                PythonRunnerFeature::Health,
                PythonRunnerFeature::Cancel,
                PythonRunnerFeature::Invoke,
            ],
            &hello.features,
        ),
        vec![PythonRunnerFeature::Health, PythonRunnerFeature::Invoke]
    );
}

#[test]
fn every_host_request_round_trips_with_at_most_once_context() {
    let invocation = PythonInvocationRequest {
        invocation_id: "invoke-7".to_owned(),
        provider: "demo".to_owned(),
        action: "lookup".to_owned(),
        arguments: json!({"query": "soma"}),
        deadline_unix_ms: 1_900_000_000_000,
        trace: Some(PythonTraceContext {
            traceparent: "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01".to_owned(),
            tracestate: Some("soma=test".to_owned()),
        }),
        actor: Some(PythonActorContext {
            actor_id: "user-1".to_owned(),
            scopes: vec!["tools:call".to_owned()],
        }),
        cancellation_token_id: "cancel-7".to_owned(),
        generation_id: "generation-3".to_owned(),
    };
    let requests = vec![
        PythonRunnerHostRequest::Describe {
            request_id: 1,
            path: "/providers/demo.py".into(),
            generation_id: "generation-3".to_owned(),
        },
        PythonRunnerHostRequest::Invoke {
            request_id: 2,
            invocation: Box::new(invocation),
        },
        PythonRunnerHostRequest::Cancel {
            request_id: 3,
            invocation_id: "invoke-7".to_owned(),
            cancellation_token_id: "cancel-7".to_owned(),
        },
        PythonRunnerHostRequest::Health { request_id: 4 },
        PythonRunnerHostRequest::Drain { request_id: 5 },
        PythonRunnerHostRequest::Shutdown { request_id: 6 },
    ];

    for request in requests {
        let frame = encode_runner_frame(&request).expect("request should frame");
        let decoded: PythonRunnerHostRequest =
            decode_runner_frame(&frame).expect("request should decode");
        assert_eq!(decoded, request);
    }
}

#[test]
fn host_calls_and_replies_keep_stable_wire_names() {
    let calls = [
        PythonRunnerHostCall::Http {
            request_id: 1,
            invocation_id: "i".to_owned(),
            request: json!({"url": "https://example.invalid"}),
        },
        PythonRunnerHostCall::Secret {
            request_id: 2,
            invocation_id: "i".to_owned(),
            name: "TOKEN".to_owned(),
        },
        PythonRunnerHostCall::StateGet {
            request_id: 3,
            invocation_id: "i".to_owned(),
            key: "cursor".to_owned(),
        },
        PythonRunnerHostCall::StatePut {
            request_id: 4,
            invocation_id: "i".to_owned(),
            key: "cursor".to_owned(),
            value: json!(7),
        },
        PythonRunnerHostCall::Log {
            request_id: 5,
            invocation_id: "i".to_owned(),
            level: "info".to_owned(),
            message: "working".to_owned(),
            fields: json!({}),
        },
        PythonRunnerHostCall::Metric {
            request_id: 6,
            invocation_id: "i".to_owned(),
            name: "items".to_owned(),
            value: serde_json::Number::from(2),
            attributes: json!({}),
        },
        PythonRunnerHostCall::Progress {
            request_id: 7,
            invocation_id: "i".to_owned(),
            current: 2,
            total: Some(3),
            message: None,
        },
    ];
    let expected = [
        "host.http",
        "host.secret",
        "host.state.get",
        "host.state.put",
        "host.log",
        "host.metric",
        "host.progress",
    ];

    for (call, expected_method) in calls.iter().zip(expected) {
        let value = serde_json::to_value(call).expect("host call JSON");
        assert_eq!(value["method"], expected_method);
        let decoded: PythonRunnerHostCall =
            serde_json::from_value(value).expect("host call should decode");
        assert_eq!(&decoded, call);
    }

    let reply = PythonRunnerReply::Health {
        request_id: 8,
        health: PythonWorkerHealth::Ready,
        generation_id: "generation-3".to_owned(),
    };
    let frame = encode_runner_frame(&reply).expect("reply frame");
    assert_eq!(
        decode_runner_frame::<PythonRunnerReply>(&frame).expect("reply decode"),
        reply
    );

    let accepted = PythonRunnerReply::Accepted {
        request_id: 9,
        invocation_id: "i".to_owned(),
        state: PythonInvocationState::Accepted,
    };
    assert_eq!(
        decode_runner_frame::<PythonRunnerReply>(
            &encode_runner_frame(&accepted).expect("accepted frame")
        )
        .expect("accepted decode"),
        accepted
    );
}

#[test]
fn stable_error_taxonomy_serializes_exact_codes() {
    let codes = [
        PythonRunnerErrorCode::PythonRuntimeMissing,
        PythonRunnerErrorCode::PythonVersionUnsupported,
        PythonRunnerErrorCode::PythonDependencyResolutionFailed,
        PythonRunnerErrorCode::PythonWorkerStartFailed,
        PythonRunnerErrorCode::PythonProtocolMismatch,
        PythonRunnerErrorCode::PythonCatalogTimeout,
        PythonRunnerErrorCode::PythonImportFailed,
        PythonRunnerErrorCode::PythonSchemaInvalid,
        PythonRunnerErrorCode::PythonPolicyDenied,
        PythonRunnerErrorCode::PythonCallTimeout,
        PythonRunnerErrorCode::PythonCallCancelled,
        PythonRunnerErrorCode::PythonWorkerCrashed,
        PythonRunnerErrorCode::PythonOutputTooLarge,
        PythonRunnerErrorCode::PythonInvalidOutput,
        PythonRunnerErrorCode::PythonNativeAbiMismatch,
    ];
    let expected = [
        "python_runtime_missing",
        "python_version_unsupported",
        "python_dependency_resolution_failed",
        "python_worker_start_failed",
        "python_protocol_mismatch",
        "python_catalog_timeout",
        "python_import_failed",
        "python_schema_invalid",
        "python_policy_denied",
        "python_call_timeout",
        "python_call_cancelled",
        "python_worker_crashed",
        "python_output_too_large",
        "python_invalid_output",
        "python_native_abi_mismatch",
    ];

    for (code, expected_code) in codes.iter().zip(expected) {
        assert_eq!(
            serde_json::to_value(code).expect("error code JSON"),
            expected_code
        );
    }

    let error = PythonRunnerReply::Error {
        request_id: 10,
        error: PythonRunnerError {
            code: PythonRunnerErrorCode::PythonWorkerCrashed,
            phase: PythonRunnerErrorPhase::Invocation,
            provider: Some("demo".to_owned()),
            source: Some("/providers/demo.py".to_owned()),
            generation_id: Some("generation-3".to_owned()),
            action: Some("lookup".to_owned()),
            retryable: false,
            public_message: "Python worker exited during execution".to_owned(),
        },
    };
    let value = serde_json::to_value(error).expect("error reply JSON");
    assert_eq!(value["error"]["code"], "python_worker_crashed");
    assert_eq!(value["error"]["phase"], "invocation");
}

#[test]
fn framed_codec_rejects_incomplete_oversized_and_malformed_frames() {
    for length in 0..4 {
        assert!(matches!(
            decode_runner_frame::<Value>(&[0_u8; 3][..length]),
            Err(PythonProtocolError::FrameHeaderTooShort { actual }) if actual == length
        ));
    }

    let oversized = ((PYTHON_RUNNER_MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
    assert!(matches!(
        decode_runner_frame::<Value>(&oversized),
        Err(PythonProtocolError::FrameTooLarge { actual, .. })
            if actual == PYTHON_RUNNER_MAX_FRAME_BYTES + 1
    ));

    let mut truncated = 5_u32.to_be_bytes().to_vec();
    truncated.extend_from_slice(br#"{}"#);
    assert!(matches!(
        decode_runner_frame::<Value>(&truncated),
        Err(PythonProtocolError::FrameLengthMismatch {
            declared: 5,
            actual: 2
        })
    ));

    let mut trailing = 2_u32.to_be_bytes().to_vec();
    trailing.extend_from_slice(br#"{}x"#);
    assert!(matches!(
        decode_runner_frame::<Value>(&trailing),
        Err(PythonProtocolError::FrameLengthMismatch {
            declared: 2,
            actual: 3
        })
    ));

    let mut invalid_json = 1_u32.to_be_bytes().to_vec();
    invalid_json.push(b'{');
    assert!(matches!(
        decode_runner_frame::<Value>(&invalid_json),
        Err(PythonProtocolError::Json(_))
    ));
}

#[test]
fn shared_python_golden_fixtures_decode_as_rust_protocol_types() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../python/tests/runner_protocol_v1.json"))
            .expect("shared runner fixture JSON");

    let hello: PythonRunnerHello =
        serde_json::from_value(fixtures["hello"].clone()).expect("hello fixture");
    assert_eq!(hello.protocol, PythonRunnerProtocolVersion::current());
    assert!(hello.features.contains(&PythonRunnerFeature::HostCalls));

    let invoke: PythonRunnerHostRequest =
        serde_json::from_value(fixtures["invoke"].clone()).expect("invoke fixture");
    assert!(matches!(
        invoke,
        PythonRunnerHostRequest::Invoke { request_id: 42, invocation }
            if invocation.invocation_id == "invoke-42"
                && invocation.generation_id == "generation-3"
    ));

    let progress: PythonRunnerHostCall =
        serde_json::from_value(fixtures["host_progress"].clone()).expect("progress fixture");
    assert!(matches!(
        progress,
        PythonRunnerHostCall::Progress {
            request_id: 43,
            current: 2,
            total: Some(3),
            ..
        }
    ));

    let error: PythonRunnerReply =
        serde_json::from_value(fixtures["error_reply"].clone()).expect("error fixture");
    assert!(matches!(
        error,
        PythonRunnerReply::Error {
            request_id: 44,
            error: PythonRunnerError {
                code: PythonRunnerErrorCode::PythonWorkerCrashed,
                retryable: false,
                ..
            }
        }
    ));
}
