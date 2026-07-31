use super::*;
use soma_provider_core::{BrokerCapability, HostCapabilities, ProviderInvocationContext};

#[test]
fn epoch_deadline_interrupts_running_guest_code() {
    let wasm = wat::parse_str(
        r#"
(module
  (memory (export "memory") 1)
  (func (export "soma_input_alloc") (param i32) (result i32) (i32.const 0))
  (func (export "soma_input_ptr") (result i32) (i32.const 0))
  (func (export "soma_call") (result i32)
    (loop $forever (br $forever))
    (i32.const 0))
  (func (export "soma_output_ptr") (result i32) (i32.const 0))
  (func (export "soma_output_len") (result i32) (i32.const 0)))
"#,
    )
    .expect("valid WAT");
    let temp = tempfile::NamedTempFile::new().expect("temporary artifact");
    std::fs::write(temp.path(), wasm).expect("write artifact");
    let runtime = WasmRuntime::new().expect("runtime");
    let capabilities = HostCapabilities::default();
    let context = ProviderInvocationContext::default();
    let started = std::time::Instant::now();
    let error = runtime
        .run(
            temp.path(),
            WasmInvocation {
                input: b"{}",
                limits: WasmRuntimeLimits {
                    timeout_ms: 20,
                    max_input_bytes: 1024,
                    max_output_bytes: 1024,
                    fuel: u64::MAX,
                    max_memory_bytes: 1024 * 1024,
                    max_table_elements: 16,
                    max_instances: 2,
                },
                capabilities: &capabilities,
                context: &context,
                resolved_hosts: BTreeMap::new(),
                deadline: Instant::now() + Duration::from_millis(20),
            },
        )
        .expect_err("epoch deadline must trap the infinite loop");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "epoch interruption must stop execution, not only its caller"
    );
    assert!(!error.is_empty());
}

#[test]
fn component_state_is_namespaced_and_requires_write_authority() {
    let runtime = WasmRuntime::new().expect("runtime");
    let mut state = WasmStoreState {
        limits: StoreLimitsBuilder::new().build(),
        capabilities: HostCapabilities {
            broker: Some(BrokerCapability {
                enabled: true,
                state_namespace: Some("provider-a".to_owned()),
                state_write: true,
                ..BrokerCapability::default()
            }),
            ..HostCapabilities::default()
        },
        state: runtime.state.clone(),
        context: ProviderInvocationContext {
            actor_id: Some("test-actor".to_owned()),
            actor_scopes: vec!["soma:read".to_owned(), "soma:write".to_owned()],
            ..ProviderInvocationContext::default()
        },
        deadline: Instant::now() + Duration::from_secs(1),
        resolved_hosts: BTreeMap::new(),
        wasi: wasmtime_wasi::WasiCtxBuilder::new().build(),
        table: wasmtime::component::ResourceTable::new(),
    };
    component_state_put(&state, "count", "2").expect("state write");
    assert_eq!(
        component_state_get(&state, "count").expect("state read"),
        "2"
    );
    state
        .capabilities
        .broker
        .as_mut()
        .expect("broker")
        .state_write = false;
    assert!(component_state_put(&state, "count", "3").is_err());
}

#[test]
fn component_network_rejects_non_public_addresses() {
    assert!(!component_public_ip("127.0.0.1".parse().unwrap()));
    assert!(!component_public_ip("10.0.0.1".parse().unwrap()));
    assert!(!component_public_ip("100.64.0.1".parse().unwrap()));
    assert!(!component_public_ip("224.0.0.1".parse().unwrap()));
    assert!(!component_public_ip("ff02::1".parse().unwrap()));
    assert!(component_public_ip("1.1.1.1".parse().unwrap()));
    assert!(component_forbidden_header("Host"));
    assert!(component_forbidden_header("proxy-authorization"));
    assert!(!component_forbidden_header("authorization"));
}

#[test]
fn verification_rejects_a_component_without_the_soma_provider_world() {
    let component = wat::parse_str("(component)").expect("valid empty component");
    let runtime = WasmRuntime::new().expect("runtime");
    let error = runtime
        .verify_component(
            &component,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .expect_err("an arbitrary component is not a Soma provider");
    assert!(error.contains("soma:provider@1.0.0"), "{error}");
}

#[test]
fn retained_artifact_survives_global_cache_eviction() {
    let runtime = WasmRuntime::new().expect("runtime");
    let deadline = Instant::now() + Duration::from_secs(5);
    let original = wat::parse_str(
        r#"
(module
  (memory (export "memory") 1)
  (data (i32.const 0) "{}")
  (func (export "soma_input_alloc") (param i32) (result i32) (i32.const 0))
  (func (export "soma_input_ptr") (result i32) (i32.const 0))
  (func (export "soma_call") (result i32) (i32.const 0))
  (func (export "soma_output_ptr") (result i32) (i32.const 0))
  (func (export "soma_output_len") (result i32) (i32.const 2)))
"#,
    )
    .expect("provider module");
    let retained = runtime
        .artifact(&original, deadline)
        .expect("compiled module");
    for index in 0..=WasmArtifactCache::MAX_ARTIFACTS {
        let bytes = wat::parse_str(format!(
            "(module (memory 1) (data (i32.const 0) \"artifact-{index}\"))"
        ))
        .expect("pressure module");
        runtime
            .artifact(&bytes, deadline)
            .expect("compiled pressure");
    }
    let capabilities = HostCapabilities::default();
    let context = ProviderInvocationContext::default();
    let output = runtime
        .run_artifact(
            retained,
            WasmInvocation {
                input: b"{}",
                limits: WasmRuntimeLimits {
                    timeout_ms: 1_000,
                    max_input_bytes: 1024,
                    max_output_bytes: 1024,
                    fuel: 100_000,
                    max_memory_bytes: 1024 * 1024,
                    max_table_elements: 16,
                    max_instances: 2,
                },
                capabilities: &capabilities,
                context: &context,
                resolved_hosts: BTreeMap::new(),
                deadline,
            },
        )
        .expect("retained artifact remains executable");
    assert_eq!(output, b"{}");
}
