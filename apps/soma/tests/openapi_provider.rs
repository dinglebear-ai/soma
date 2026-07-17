use std::{fs, path::Path};

use serde_json::json;
use soma_contracts::{
    provider_validation::validate_provider_manifest_value, providers::CapabilityGrant,
};
use soma_provider_adapters::openapi::OpenApiProvider;
use soma_provider_core::{Provider as CoreProvider, ProviderCall as CoreProviderCall};
use soma_service::{
    capabilities::CapabilityBroker,
    provider_registry::{
        ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRegistry, ProviderRequestLimits,
        ProviderSurface, SharedAdapter,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn openapi_catalog() -> soma_contracts::providers::ProviderCatalog {
    let path =
        workspace_root().join("docs/contracts/examples/provider-manifests/openapi.valid.json");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("fixture should exist"))
            .expect("fixture JSON");
    validate_provider_manifest_value(&value).expect("valid OpenAPI fixture")
}

#[tokio::test]
async fn openapi_provider_network_is_default_denied_before_execution() {
    let provider = SharedAdapter::wrap(OpenApiProvider::arc(openapi_catalog()));
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");
    let error = registry
        .dispatch(call())
        .await
        .expect_err("network should be denied before provider code");

    assert_eq!(&*error.code, "capability_denied");
}

#[tokio::test]
async fn openapi_provider_executes_pinned_local_operation() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local test server");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept request");
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.expect("read request");
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("POST /upstream/weather HTTP/1.1"));
        assert!(request.contains(r#"{"city":"Paris"}"#));
        let body = r#"{"city":"Paris","celsius":21}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let mut catalog = openapi_catalog();
    catalog.meta = json!({
        "openapi": {
            "base_url": format!("http://{}", addr)
        }
    });
    catalog
        .capabilities
        .network
        .as_mut()
        .expect("fixture declares network")
        .allowed_hosts = vec!["127.0.0.1".to_owned()];
    catalog.tools[0].meta = json!({
        "openapi": {
            "method": "POST",
            "path": "/upstream/weather"
        }
    });
    let provider = SharedAdapter::wrap(OpenApiProvider::arc(catalog));
    let registry = ProviderRegistry::with_capabilities(
        vec![provider],
        CapabilityBroker::new(vec![CapabilityGrant::Network {
            allowed_hosts: vec!["127.0.0.1".to_owned()],
        }]),
    )
    .expect("registry");
    let output = registry
        .dispatch(call())
        .await
        .expect("OpenAPI provider should call pinned local operation");

    assert_eq!(output.value, json!({"city": "Paris", "celsius": 21}));
    server.await.expect("server task");
}

#[tokio::test]
async fn openapi_provider_rejects_absolute_operation_urls() {
    let mut catalog = openapi_catalog();
    catalog.meta = json!({ "openapi": { "base_url": "http://127.0.0.1:1" } });
    catalog
        .capabilities
        .network
        .as_mut()
        .expect("fixture declares network")
        .allowed_hosts = vec!["127.0.0.1".to_owned()];
    catalog.tools[0].meta = json!({
        "openapi": {
            "method": "POST",
            "path": "http://169.254.169.254/latest/meta-data"
        }
    });
    let provider = SharedAdapter::wrap(OpenApiProvider::arc(catalog));
    let registry = ProviderRegistry::with_capabilities(
        vec![provider],
        CapabilityBroker::new(vec![CapabilityGrant::Network {
            allowed_hosts: vec!["127.0.0.1".to_owned()],
        }]),
    )
    .expect("registry");
    let error = registry
        .dispatch(call())
        .await
        .expect_err("absolute operation URLs should be rejected");

    assert_eq!(&*error.code, "openapi_absolute_operation_url_denied");
}

/// `validate_base_url` must fail closed: an OpenAPI provider always makes a
/// network call, so a manifest that omits `capabilities.network` entirely
/// must not be treated as "no restriction needed." Calls the raw
/// `soma_provider_core::Provider` directly (bypassing soma-service's
/// `CapabilityBroker`, which enforces a *different* layer of this same
/// grant) so this assertion is specifically about the adapter's own
/// `validate_base_url` defense, not the broker's.
#[tokio::test]
async fn openapi_provider_denies_dispatch_when_network_capability_is_absent() {
    let mut catalog = openapi_catalog();
    catalog.capabilities.network = None;
    catalog.meta = json!({ "openapi": { "base_url": "http://127.0.0.1:1" } });

    let error = OpenApiProvider::arc(catalog)
        .call(CoreProviderCall {
            provider: "weather-openapi".to_owned(),
            action: "weather-current".to_owned(),
            params: json!({"city": "Paris"}),
            surface: soma_provider_core::ProviderSurface::Mcp,
            snapshot_id: "test-snapshot".to_owned(),
        })
        .await
        .expect_err("no network capability declared should deny dispatch");

    assert_eq!(&*error.code, "openapi_network_capability_required");
}

/// Same as above, but for a manifest that declares `capabilities.network`
/// with `enabled: false` — this must also fail closed rather than skip the
/// allowlist check.
#[tokio::test]
async fn openapi_provider_denies_dispatch_when_network_capability_is_disabled() {
    let mut catalog = openapi_catalog();
    catalog
        .capabilities
        .network
        .as_mut()
        .expect("fixture declares network")
        .enabled = false;
    catalog.meta = json!({ "openapi": { "base_url": "http://127.0.0.1:1" } });

    let error = OpenApiProvider::arc(catalog)
        .call(CoreProviderCall {
            provider: "weather-openapi".to_owned(),
            action: "weather-current".to_owned(),
            params: json!({"city": "Paris"}),
            surface: soma_provider_core::ProviderSurface::Mcp,
            snapshot_id: "test-snapshot".to_owned(),
        })
        .await
        .expect_err("disabled network capability should deny dispatch");

    assert_eq!(&*error.code, "openapi_network_capability_required");
}

/// `call.params` must be a JSON object for every HTTP method, not only
/// GET/DELETE as in the pre-delegation implementation (see the module doc
/// on `soma_provider_adapters::openapi`).
#[tokio::test]
async fn openapi_provider_rejects_non_object_params_for_post() {
    let mut catalog = openapi_catalog();
    catalog.meta = json!({ "openapi": { "base_url": "http://127.0.0.1:1" } });
    catalog
        .capabilities
        .network
        .as_mut()
        .expect("fixture declares network")
        .allowed_hosts = vec!["127.0.0.1".to_owned()];
    catalog.tools[0].meta = json!({
        "openapi": { "method": "POST", "path": "/upstream/weather" }
    });

    let error = OpenApiProvider::arc(catalog)
        .call(CoreProviderCall {
            provider: "weather-openapi".to_owned(),
            action: "weather-current".to_owned(),
            params: json!(["not", "an", "object"]),
            surface: soma_provider_core::ProviderSurface::Mcp,
            snapshot_id: "test-snapshot".to_owned(),
        })
        .await
        .expect_err("non-object params should be rejected for POST");

    assert_eq!(&*error.code, "openapi_params_must_be_object");
}

/// `{name}` placeholders in a declared operation `path` are honored as path
/// parameters end-to-end through the adapter (previously inert literal
/// text — see the module doc's documented behavior deltas).
#[tokio::test]
async fn openapi_provider_substitutes_path_parameters() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local test server");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept request");
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.expect("read request");
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("POST /upstream/Paris HTTP/1.1"));
        let body = r#"{"city":"Paris","celsius":21}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let mut catalog = openapi_catalog();
    catalog.meta = json!({ "openapi": { "base_url": format!("http://{}", addr) } });
    catalog
        .capabilities
        .network
        .as_mut()
        .expect("fixture declares network")
        .allowed_hosts = vec!["127.0.0.1".to_owned()];
    catalog.tools[0].meta = json!({
        "openapi": { "method": "POST", "path": "/upstream/{city}" }
    });

    let output = OpenApiProvider::arc(catalog)
        .call(CoreProviderCall {
            provider: "weather-openapi".to_owned(),
            action: "weather-current".to_owned(),
            params: json!({"city": "Paris"}),
            surface: soma_provider_core::ProviderSurface::Mcp,
            snapshot_id: "test-snapshot".to_owned(),
        })
        .await
        .expect("path parameter should be substituted and request should succeed");

    assert_eq!(output.value, json!({"city": "Paris", "celsius": 21}));
    server.await.expect("server task");
}

fn call() -> ProviderCall {
    ProviderCall {
        provider: String::new(),
        action: "weather-current".to_owned(),
        params: json!({"city": "Paris"}),
        principal: ProviderPrincipal::loopback_dev(),
        auth_mode: ProviderAuthMode::LoopbackDev,
        surface: ProviderSurface::Mcp,
        destructive_confirmed: false,
        limits: ProviderRequestLimits::default(),
        snapshot_id: String::new(),
    }
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}
