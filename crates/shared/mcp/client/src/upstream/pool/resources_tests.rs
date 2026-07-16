use crate::config::UpstreamConfig;
use crate::upstream::pool::{InProcessUpstream, UpstreamPool};
use crate::upstream::{ResourceDescriptor, TransportKind, UpstreamSnapshot};

#[tokio::test]
async fn resources_obey_proxy_flag_and_filters() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "local".to_owned(),
        proxy_resources: true,
        expose_resources: Some(vec!["file://allowed/*".to_owned()]),
        ..UpstreamConfig::default()
    };
    let mut snapshot = UpstreamSnapshot::empty("local", TransportKind::InProcess);
    snapshot.resources.push(ResourceDescriptor {
        uri: "file://allowed/one".to_owned(),
        name: None,
    });
    snapshot.resources.push(ResourceDescriptor {
        uri: "file://denied/two".to_owned(),
        name: None,
    });

    pool.register_in_process(
        config,
        InProcessUpstream::new("local").with_snapshot(snapshot),
    )
    .unwrap();

    let resources = pool.list_resources("local").await.unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, "file://allowed/one");
}

#[tokio::test]
async fn resources_return_empty_when_proxy_disabled() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "local".to_owned(),
        proxy_resources: false,
        ..UpstreamConfig::default()
    };
    let mut snapshot = UpstreamSnapshot::empty("local", TransportKind::InProcess);
    snapshot.resources.push(ResourceDescriptor {
        uri: "file://allowed/one".to_owned(),
        name: None,
    });

    pool.register_in_process(
        config,
        InProcessUpstream::new("local").with_snapshot(snapshot),
    )
    .unwrap();

    assert!(pool.list_resources("local").await.unwrap().is_empty());
}
