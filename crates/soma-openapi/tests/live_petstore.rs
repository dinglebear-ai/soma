use std::time::Duration;

use soma_openapi::{OpenApiConfig, OpenApiRegistry, OpenApiSpecConfig, SpecSource};

#[tokio::test]
#[ignore = "network: hits the public Swagger Petstore"]
async fn petstore_live_load_and_dispatch() {
    let cfg = OpenApiConfig {
        specs: vec![OpenApiSpecConfig {
            label: "petstore".into(),
            spec_source: SpecSource::Url(
                "https://petstore3.swagger.io/api/v3/openapi.json"
                    .parse()
                    .unwrap(),
            ),
            base_url: "https://petstore3.swagger.io/api/v3".parse().unwrap(),
            allowed_operations: vec!["findPetsByStatus".into()],
            credential: None,
        }],
    };

    let registry = OpenApiRegistry::from_config(cfg, Duration::from_secs(10)).await;
    registry
        .operation("petstore", "findPetsByStatus")
        .expect("allowlisted operation present");
    assert_eq!(
        registry
            .operation("petstore", "getInventory")
            .unwrap_err()
            .kind(),
        "unknown_action"
    );

    let client = soma_openapi::http::build_dispatch_client().expect("dispatch client");
    let out = soma_openapi::dispatch_openapi_call(
        &registry,
        &client,
        "petstore",
        "findPetsByStatus",
        serde_json::json!({ "status": "available" }),
    )
    .await
    .expect("live dispatch to petstore");
    assert!(out.as_array().is_some());
}

#[tokio::test]
async fn private_base_url_is_rejected_without_network() {
    let cfg = OpenApiConfig {
        specs: vec![OpenApiSpecConfig {
            label: "internal".into(),
            spec_source: SpecSource::Url("https://10.0.0.5/openapi.json".parse().unwrap()),
            base_url: "https://10.0.0.5/api".parse().unwrap(),
            allowed_operations: vec!["anything".into()],
            credential: None,
        }],
    };
    let registry = OpenApiRegistry::from_config(cfg, Duration::from_secs(3)).await;
    assert!(!registry.labels().contains(&"internal".to_string()));
}
