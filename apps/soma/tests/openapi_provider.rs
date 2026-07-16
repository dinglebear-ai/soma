use std::{fs, path::Path};

use serde_json::json;
use soma_contracts::{
    provider_validation::validate_provider_manifest_value, providers::CapabilityGrant,
};
use soma_service::{
    capabilities::CapabilityBroker,
    provider_registry::{
        ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRegistry, ProviderRequestLimits,
        ProviderSurface,
    },
    providers::openapi::OpenApiProvider,
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
    let provider = OpenApiProvider::arc(openapi_catalog());
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
    let provider = OpenApiProvider::arc(catalog);
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
    let provider = OpenApiProvider::arc(catalog);
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
