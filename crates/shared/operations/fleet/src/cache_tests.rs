use super::*;
use crate::{HostEndpoint, SshEndpoint, TopologySnapshot};

fn local(name: &str) -> HostRecord {
    HostRecord::new(HostId::new(name).unwrap(), HostEndpoint::Local)
}

fn ssh(name: &str, user: &str) -> HostRecord {
    HostRecord::new(
        HostId::new(name).unwrap(),
        HostEndpoint::Ssh(SshEndpoint::new(name).unwrap().with_user(user).unwrap()),
    )
}

#[test]
fn cache_reuses_only_exact_topology_revisions() {
    let cache = ConnectionCache::new();
    let first = ssh("devhost", "devuser");
    let changed = ssh("devhost", "root");
    cache.insert(&first, "session-a".to_owned());

    assert_eq!(cache.get(&first).as_deref(), Some(&"session-a".to_owned()));
    assert!(cache.get(&changed).is_none());
    assert_eq!(cache.len(), 1);
    assert!(!cache.is_empty());
}

#[test]
fn host_invalidation_removes_every_revision() {
    let cache = ConnectionCache::new();
    let first = ssh("devhost", "devuser");
    let changed = ssh("devhost", "root");
    cache.insert(&first, 1);
    cache.insert(&changed, 2);
    cache.insert(&local("edgehost"), 3);

    let removed = cache.invalidate_host(first.id());
    assert_eq!(removed.len(), 2);
    assert_eq!(cache.len(), 1);
    assert!(
        cache
            .keys()
            .iter()
            .all(|key| key.host().as_str() == "edgehost")
    );
}

#[test]
fn retaining_snapshot_evicts_missing_and_stale_keys() {
    let cache = ConnectionCache::new();
    let old_devhost = ssh("devhost", "devuser");
    let new_devhost = ssh("devhost", "root");
    let edgehost = local("edgehost");
    cache.insert(&old_devhost, "old");
    cache.insert(&new_devhost, "new");
    cache.insert(&edgehost, "removed-host");

    let snapshot = TopologySnapshot::new([new_devhost.clone()]).unwrap();
    let removed = cache.retain_snapshot(&snapshot);
    assert_eq!(removed.len(), 2);
    assert_eq!(cache.get(&new_devhost).as_deref(), Some(&"new"));
    assert_eq!(cache.len(), 1);
    assert!(cache.remove(&new_devhost).is_some());
    assert!(cache.is_empty());
}
