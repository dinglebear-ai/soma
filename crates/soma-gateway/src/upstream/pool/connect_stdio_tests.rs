use super::*;

#[test]
fn stdio_connection_plan_runs_spawn_and_env_validation() {
    let cfg = UpstreamConfig {
        name: "demo".to_owned(),
        command: Some("node".to_owned()),
        args: vec!["server.js".to_owned()],
        env: [("UPSTREAM_TOKEN".to_owned(), "secret".to_owned())].into(),
        ..UpstreamConfig::default()
    };
    let spec = plan_stdio_connection(&cfg, &SpawnGuard::default()).unwrap();
    assert_eq!(spec.command, "node");
}

#[test]
fn stdio_connection_plan_rejects_path_poisoning() {
    let cfg = UpstreamConfig {
        name: "demo".to_owned(),
        command: Some("/tmp/x/node".to_owned()),
        ..UpstreamConfig::default()
    };
    assert!(plan_stdio_connection(&cfg, &SpawnGuard::default()).is_err());
}
