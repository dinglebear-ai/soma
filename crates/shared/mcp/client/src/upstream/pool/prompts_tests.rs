use crate::config::UpstreamConfig;
use crate::upstream::pool::{InProcessUpstream, UpstreamPool};
use crate::upstream::{PromptDescriptor, TransportKind, UpstreamSnapshot};

#[tokio::test]
async fn prompts_obey_proxy_flag_and_filters() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "local".to_owned(),
        expose_prompts: Some(vec!["assist*".to_owned()]),
        ..UpstreamConfig::default()
    };
    let mut snapshot = UpstreamSnapshot::empty("local", TransportKind::InProcess);
    snapshot.prompts.push(PromptDescriptor {
        name: "assist".to_owned(),
        description: None,
    });
    snapshot.prompts.push(PromptDescriptor {
        name: "admin".to_owned(),
        description: None,
    });

    pool.register_in_process(
        config,
        InProcessUpstream::new("local").with_snapshot(snapshot),
    )
    .unwrap();

    let prompts = pool.list_prompts("local").await.unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].name, "assist");
}

#[tokio::test]
async fn prompts_return_empty_when_proxy_disabled() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "local".to_owned(),
        proxy_prompts: false,
        ..UpstreamConfig::default()
    };
    let mut snapshot = UpstreamSnapshot::empty("local", TransportKind::InProcess);
    snapshot.prompts.push(PromptDescriptor {
        name: "assist".to_owned(),
        description: None,
    });

    pool.register_in_process(
        config,
        InProcessUpstream::new("local").with_snapshot(snapshot),
    )
    .unwrap();

    assert!(pool.list_prompts("local").await.unwrap().is_empty());
}
