use serde_json::Value;

use super::*;
use crate::{HostId, HostRecord};

fn ssh(user: &str) -> HostEndpoint {
    HostEndpoint::Ssh(
        SshEndpoint::new("198.51.100.10")
            .unwrap()
            .with_user(user)
            .unwrap()
            .with_known_hosts_file("/home/devuser/.ssh/known_hosts")
            .unwrap(),
    )
}

#[test]
fn endpoint_changes_derive_new_topology_revisions() {
    let id = HostId::new("devhost").unwrap();
    let first = HostRecord::new(id.clone(), ssh("devuser"));
    let second = HostRecord::new(id, ssh("root"));
    assert_ne!(first.revision(), second.revision());
    assert_ne!(first.pool_key(), second.pool_key());
}

#[test]
fn host_round_trip_rejects_forged_revision() {
    let host = HostRecord::new(HostId::new("devhost").unwrap(), ssh("devuser"));
    let encoded = serde_json::to_value(&host).unwrap();
    assert_eq!(
        serde_json::from_value::<HostRecord>(encoded.clone()).unwrap(),
        host
    );

    let mut forged = encoded;
    forged["revision"] = Value::String("0".repeat(64));
    assert!(serde_json::from_value::<HostRecord>(forged).is_err());
}

#[test]
fn endpoints_reject_ambient_or_credential_bearing_configuration() {
    assert!(SshEndpoint::new("devhost").unwrap().with_port(0).is_err());
    assert!(
        SshEndpoint::new("devhost")
            .unwrap()
            .with_identity_file(".ssh/id_ed25519")
            .is_err()
    );
    assert!(HttpEndpoint::new("ftp://devhost").is_err());
    assert!(HttpEndpoint::new("https://user:pass@devhost").is_err());
    assert!(HttpEndpoint::new("https://devhost.example").is_ok());
}
