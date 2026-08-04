use super::*;

#[tokio::test]
async fn default_topology_and_repository_are_stable() {
    let snapshot = topology(&SynapseConfig::default()).unwrap();
    assert_eq!(snapshot.len(), 1);
    let repository = StaticHostRepository::new(snapshot.clone());
    assert_eq!(repository.snapshot().await.unwrap(), snapshot);
}

#[tokio::test]
async fn routed_executor_keeps_http_endpoints_fail_closed() {
    let executor = RoutedCommandExecutor::new(Arc::new(OpenSshDriver::default()));
    let host = HostRecord::new(
        HostId::new("remote-http").unwrap(),
        HostEndpoint::Http(soma_fleet::HttpEndpoint::new("https://example.com").unwrap()),
    );
    let request = CommandRequest::new(
        "true",
        Vec::<String>::new(),
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 1_000),
    )
    .unwrap();
    let error = executor
        .execute(&host, &request, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("HTTP fleet endpoints"));
}
