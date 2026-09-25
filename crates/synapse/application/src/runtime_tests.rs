use serde_json::{Value, json};
use soma_ops::OperationName;
use tokio_util::sync::CancellationToken;

use crate::ExecutionError;
use crate::runtime_test_support::runtime;

#[tokio::test(flavor = "current_thread")]
async fn every_canonical_read_operation_executes_and_validates() {
    let runtime = runtime();
    let cancellation = CancellationToken::new();
    for (name, parameters) in cases() {
        let operation = OperationName::new(name).unwrap();
        runtime
            .execute(&operation, &parameters, &cancellation)
            .await
            .unwrap_or_else(|error| panic!("{operation} failed: {error}"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn mutation_operations_are_not_smuggled_through_read_runtime() {
    let operation = OperationName::new("container.restart").unwrap();
    let result = runtime()
        .execute(
            &operation,
            &json!({"container_id":"abc"}),
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(result, Err(ExecutionError::UnsupportedOperation(name)) if name == operation));
}

fn cases() -> Vec<(&'static str, Value)> {
    vec![
        ("product.help", json!({})),
        ("docker.info", json!({})),
        ("docker.df", json!({})),
        ("docker.images", json!({})),
        ("docker.networks", json!({})),
        ("docker.volumes", json!({})),
        ("container.list", json!({})),
        ("container.inspect", json!({"container_id":"abc"})),
        ("container.logs", json!({"container_id":"abc"})),
        ("container.stats", json!({"container_id":"abc"})),
        ("container.top", json!({"container_id":"abc"})),
        ("container.search", json!({"query":"soma"})),
        ("host.status", json!({})),
        ("host.info", json!({})),
        ("host.uptime", json!({})),
        ("host.resources", json!({})),
        ("host.services", json!({"host":"devhost"})),
        ("host.network", json!({})),
        ("host.mounts", json!({"host":"devhost"})),
        ("host.ports", json!({"host":"devhost"})),
        ("host.doctor", json!({"host":"devhost"})),
        ("compose.list", json!({"host":"devhost"})),
        ("compose.status", json!({"host":"devhost","project":"soma"})),
        ("compose.logs", json!({"host":"devhost","project":"soma"})),
        ("compose.refresh", json!({"host":"devhost"})),
        ("fleet.nodes", json!({})),
        ("files.read", json!({"host":"devhost","path":"/srv/a.txt"})),
        (
            "files.find",
            json!({"host":"devhost","path":"/srv","pattern":"*.log"}),
        ),
        ("processes.list", json!({"host":"devhost"})),
        ("filesystem.usage", json!({"host":"devhost"})),
        (
            "files.compare",
            json!({"source_host":"devhost","source_path":"/srv/a.txt","content":"hello\n"}),
        ),
        ("zfs.pools", json!({"host":"devhost"})),
        ("zfs.datasets", json!({"host":"devhost"})),
        ("zfs.snapshots", json!({"host":"devhost"})),
        ("logs.syslog", json!({"host":"devhost"})),
        ("logs.journal", json!({"host":"devhost"})),
        ("logs.kernel", json!({"host":"devhost"})),
        ("logs.auth", json!({"host":"devhost"})),
    ]
}
