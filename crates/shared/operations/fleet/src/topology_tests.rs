use super::*;
use crate::{CapabilityName, HostEndpoint, SshEndpoint};

fn ssh(user: &str) -> HostEndpoint {
    HostEndpoint::Ssh(
        SshEndpoint::new("198.51.100.10")
            .unwrap()
            .with_user(user)
            .unwrap(),
    )
}

#[test]
fn host_labels_and_capabilities_are_sorted_unique() {
    let host = HostRecord::new(HostId::new("devhost").unwrap(), ssh("jmagar"))
        .with_label("linux")
        .unwrap()
        .with_label("linux")
        .unwrap()
        .with_capability(CapabilityName::new("transport.ssh").unwrap());
    assert_eq!(host.labels().collect::<Vec<_>>(), vec!["linux"]);
    assert_eq!(
        host.capabilities()
            .map(CapabilityName::as_str)
            .collect::<Vec<_>>(),
        vec!["transport.ssh"]
    );
}

#[test]
fn topology_is_sorted_unique_and_revision_bound() {
    let alpha = HostRecord::new(HostId::new("alpha").unwrap(), HostEndpoint::Local);
    let beta = HostRecord::new(HostId::new("beta").unwrap(), ssh("jmagar"));
    let snapshot = TopologySnapshot::new([beta.clone(), alpha.clone()]).unwrap();
    assert_eq!(
        snapshot
            .hosts()
            .map(|host| host.id().as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(snapshot.len(), 2);
    assert!(!snapshot.is_empty());
    assert_eq!(snapshot.get(beta.id()), Some(&beta));
    assert!(TopologySnapshot::new([alpha.clone(), alpha]).is_err());
}

#[test]
fn snapshot_revision_changes_with_endpoint_material() {
    let first = TopologySnapshot::new([HostRecord::new(
        HostId::new("devhost").unwrap(),
        ssh("jmagar"),
    )])
    .unwrap();
    let second = TopologySnapshot::new([HostRecord::new(
        HostId::new("devhost").unwrap(),
        ssh("root"),
    )])
    .unwrap();
    assert_ne!(first.revision(), second.revision());
}
