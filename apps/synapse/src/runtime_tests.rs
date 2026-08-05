use super::*;

#[tokio::test]
async fn default_runtime_executes_product_help_through_canonical_read_engine() {
    let runtime = StandaloneRuntime::from_config(SynapseConfig::default()).unwrap();
    let value = runtime
        .execute(
            "product.help",
            &serde_json::json!({}),
            &ExecuteOptions::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(
        value["operations"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
}

#[tokio::test]
async fn mutations_return_the_exact_plan_until_confirmed() {
    let runtime = StandaloneRuntime::from_config(SynapseConfig::default()).unwrap();
    let error = runtime
        .execute(
            "container.start",
            &serde_json::json!({"host":"local","container_id":"missing"}),
            &ExecuteOptions::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(error.plan().is_some());
}
