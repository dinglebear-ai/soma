use std::path::Path;
use std::time::Duration;

use super::*;
use crate::SshEndpoint;

fn ssh_host() -> HostRecord {
    HostRecord::new(
        HostId::new("devhost").unwrap(),
        HostEndpoint::Ssh(
            SshEndpoint::new("198.51.100.10")
                .unwrap()
                .with_port(2222)
                .unwrap()
                .with_user("jmagar")
                .unwrap()
                .with_identity_file("/home/jmagar/.ssh/id_ed25519")
                .unwrap()
                .with_config_file("/home/jmagar/.ssh/config")
                .unwrap()
                .with_known_hosts_file("/home/jmagar/.ssh/known_hosts")
                .unwrap(),
        ),
    )
}

#[test]
fn connection_plan_is_strict_and_revision_bound() {
    let host = ssh_host();
    let connector =
        OpenSshConnector::new(Duration::from_secs(3), Duration::from_secs(10), 2).unwrap();
    let plan = connector.plan(&host).unwrap();
    assert_eq!(plan.host(), host.id());
    assert_eq!(plan.revision(), host.revision());
    assert_eq!(plan.destination(), "198.51.100.10");
    assert_eq!(plan.port(), 2222);
    assert_eq!(plan.user(), Some("jmagar"));
    assert_eq!(
        plan.identity_file(),
        Some(Path::new("/home/jmagar/.ssh/id_ed25519"))
    );
    assert_eq!(
        plan.config_file(),
        Some(Path::new("/home/jmagar/.ssh/config"))
    );
    assert_eq!(
        plan.known_hosts_file(),
        Some(Path::new("/home/jmagar/.ssh/known_hosts"))
    );
    assert_eq!(plan.connect_timeout(), Duration::from_secs(3));
    assert_eq!(plan.server_alive_interval(), Duration::from_secs(10));
    assert!(plan.strict_known_hosts());
}

#[test]
fn connector_rejects_invalid_bounds_and_non_ssh_hosts() {
    assert!(OpenSshConnector::new(Duration::ZERO, Duration::from_secs(1), 1).is_err());
    let local = HostRecord::new(HostId::new("local").unwrap(), HostEndpoint::Local);
    assert!(OpenSshConnector::default().plan(&local).is_err());
}
