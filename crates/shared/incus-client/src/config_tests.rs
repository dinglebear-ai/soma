use super::*;

#[test]
fn unix_socket_stores_the_given_path() {
    let config = ClientConfig::unix_socket("/var/lib/incus/unix.socket");
    assert_eq!(
        config.socket_path,
        PathBuf::from("/var/lib/incus/unix.socket")
    );
}

#[test]
fn unix_socket_accepts_owned_pathbuf_and_str() {
    let from_str = ClientConfig::unix_socket("/tmp/a.sock");
    let from_pathbuf = ClientConfig::unix_socket(PathBuf::from("/tmp/a.sock"));
    assert_eq!(from_str, from_pathbuf);
}

#[test]
fn unix_socket_defaults_to_a_30_second_request_timeout() {
    let config = ClientConfig::unix_socket("/tmp/a.sock");
    assert_eq!(config.request_timeout, Some(Duration::from_secs(30)));
}

#[test]
fn with_request_timeout_overrides_the_default() {
    let config = ClientConfig::unix_socket("/tmp/a.sock")
        .with_request_timeout(Some(Duration::from_millis(500)));
    assert_eq!(config.request_timeout, Some(Duration::from_millis(500)));

    let unbounded = ClientConfig::unix_socket("/tmp/a.sock").with_request_timeout(None);
    assert_eq!(unbounded.request_timeout, None);
}
