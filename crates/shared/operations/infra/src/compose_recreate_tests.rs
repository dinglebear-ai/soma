use std::collections::BTreeMap;

use soma_fleet::{HostId, TopologyRevision};

use super::*;
use crate::{ComposeServiceConfig, ComposeServiceStatus};

#[test]
fn compose_fingerprint_is_deterministic() {
    let revision = TopologyRevision::from_material(b"test");
    let config = ComposeConfig {
        host: HostId::new("host").unwrap(),
        topology_revision: revision.clone(),
        project: "soma".into(),
        services: BTreeMap::from([(
            "api".into(),
            ComposeServiceConfig {
                image: Some("api:v1".into()),
                build_context: None,
                profiles: Vec::new(),
            },
        )]),
        networks: Vec::new(),
        volumes: Vec::new(),
    };
    let status = ComposeStatus {
        host: HostId::new("host").unwrap(),
        topology_revision: revision,
        project: "soma".into(),
        services: vec![ComposeServiceStatus {
            service: "api".into(),
            container_name: Some("soma-api-1".into()),
            state: Some("running".into()),
            health: None,
            exit_code: Some(0),
            image: Some("api:v1".into()),
        }],
    };
    assert_eq!(
        compose_recreate_fingerprint(&config, &status).unwrap(),
        compose_recreate_fingerprint(&config, &status).unwrap()
    );
}
