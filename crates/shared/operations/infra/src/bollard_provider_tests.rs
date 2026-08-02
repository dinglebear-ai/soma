use soma_fleet::{HostEndpoint, SshEndpoint};

use super::*;

fn provider() -> BollardClientProvider {
    BollardClientProvider::new(Arc::new(ConnectionPool::new(Arc::new(
        OpenSshConnector::default(),
    ))))
}

#[test]
fn socket_plans_follow_endpoint_kind_and_defaults() {
    let local = HostRecord::new(HostId::new("local").unwrap(), HostEndpoint::Local);
    assert!(matches!(
        provider().plan(&local).unwrap(),
        SocketPlan::Local(None)
    ));

    let remote = HostRecord::new(
        HostId::new("remote").unwrap(),
        HostEndpoint::Ssh(SshEndpoint::new("remote").unwrap()),
    );
    match provider().plan(&remote).unwrap() {
        SocketPlan::Remote(path) => assert_eq!(path, Path::new(DEFAULT_REMOTE_SOCKET)),
        SocketPlan::Local(_) => panic!("expected remote plan"),
    }
}

#[test]
fn socket_paths_reject_relative_and_parent_components() {
    let id = HostId::new("dookie").unwrap();
    assert!(
        provider()
            .with_local_socket(id.clone(), "docker.sock")
            .is_err()
    );
    assert!(
        provider()
            .with_remote_socket(id, "/var/run/../docker.sock")
            .is_err()
    );
}
