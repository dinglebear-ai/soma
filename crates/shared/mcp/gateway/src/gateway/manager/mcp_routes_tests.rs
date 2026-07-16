use crate::config::{GatewayConfig, UpstreamConfig};
use crate::upstream::pool::{InProcessUpstream, UpstreamPool};
use crate::upstream::{
    PromptDescriptor, ResourceDescriptor, ToolDescriptor, TransportKind, UpstreamSnapshot,
};

use super::*;

#[tokio::test]
async fn tool_routes_use_native_names_when_unique() {
    let manager = manager_with_pool(pool_with_snapshots(vec![snapshot(
        "one",
        vec![tool("echo")],
        vec![],
        vec![],
    )]));

    let routes = manager.tool_routes().await.expect("routes");

    assert_eq!(routes[0].name, "echo");
    assert_eq!(routes[0].upstream, "one");
    assert_eq!(routes[0].native_name, "echo");
}

#[tokio::test]
async fn tool_routes_namespace_duplicate_names_without_gateway_reserved_words() {
    let manager = manager_with_pool(pool_with_snapshots(vec![
        snapshot("alpha", vec![tool("soma"), tool("search")], vec![], vec![]),
        snapshot("beta", vec![tool("search")], vec![], vec![]),
    ]));

    let names: Vec<String> = manager
        .tool_routes()
        .await
        .expect("routes")
        .into_iter()
        .map(|route| route.name)
        .collect();

    assert_eq!(names, vec!["soma", "alpha__search", "beta__search"]);
}

#[tokio::test]
async fn resource_routes_round_trip_native_uris() {
    let native = "test://one/path?x=1&space=a b";
    let manager = manager_with_pool(pool_with_snapshots(vec![snapshot(
        "up.one",
        vec![],
        vec![resource(native)],
        vec![],
    )]));

    let routes = manager.resource_routes().await.expect("resources");
    let parsed = parse_upstream_resource_uri(&routes[0].uri).expect("synthetic route parses");

    assert_eq!(routes[0].upstream, "up.one");
    assert_eq!(routes[0].native_uri, native);
    assert_eq!(parsed, ("up.one".to_owned(), native.to_owned()));
}

#[tokio::test]
async fn prompt_routes_namespace_duplicate_names() {
    let manager = manager_with_pool(pool_with_snapshots(vec![
        snapshot("one", vec![], vec![], vec![prompt("help")]),
        snapshot("two", vec![], vec![], vec![prompt("help")]),
    ]));

    let names: Vec<String> = manager
        .prompt_routes()
        .await
        .expect("prompts")
        .into_iter()
        .map(|route| route.name)
        .collect();

    assert_eq!(names, vec!["one__help", "two__help"]);
}

fn manager_with_pool(pool: UpstreamPool) -> GatewayManager {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    manager.install_pool_for_tests(pool);
    manager
}

fn pool_with_snapshots(snapshots: Vec<UpstreamSnapshot>) -> UpstreamPool {
    let pool = UpstreamPool::default();
    for snapshot in snapshots {
        let name = snapshot.name.clone();
        pool.register_in_process(
            UpstreamConfig {
                name: name.clone(),
                ..UpstreamConfig::default()
            },
            InProcessUpstream::new(name).with_snapshot(snapshot),
        )
        .expect("register in-process upstream");
    }
    pool
}

fn snapshot(
    name: &str,
    tools: Vec<ToolDescriptor>,
    resources: Vec<ResourceDescriptor>,
    prompts: Vec<PromptDescriptor>,
) -> UpstreamSnapshot {
    let mut snapshot = UpstreamSnapshot::empty(name, TransportKind::InProcess);
    snapshot.tools = tools;
    snapshot.resources = resources;
    snapshot.prompts = prompts;
    snapshot
}

fn tool(name: &str) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_owned(),
        description: None,
        input_schema: Some(serde_json::json!({"type": "object"})),
        output_schema: None,
        destructive: false,
    }
}

fn resource(uri: &str) -> ResourceDescriptor {
    ResourceDescriptor {
        uri: uri.to_owned(),
        name: Some("resource".to_owned()),
    }
}

fn prompt(name: &str) -> PromptDescriptor {
    PromptDescriptor {
        name: name.to_owned(),
        description: Some("prompt".to_owned()),
    }
}
