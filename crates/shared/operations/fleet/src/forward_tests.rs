use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;

use super::*;
use crate::{HostEndpoint, HostId};

fn local_host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

#[test]
fn generated_forward_paths_do_not_include_host_aliases() {
    let host = HostRecord::new(HostId::new("host-with-alias").unwrap(), HostEndpoint::Local);
    let path = forwarded_socket_path(&host).unwrap();
    assert!(!path.to_string_lossy().contains("host-with-alias"));
    assert!(path.parent().unwrap().ends_with("soma-fleet/forward"));
}

#[tokio::test(flavor = "current_thread")]
async fn secure_socket_requires_real_owned_socket_and_applies_mode() {
    let directory = secure_runtime_subdir("forward-test").unwrap();
    let socket = directory.join("test.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    secure_socket(&socket, &local_host()).await.unwrap();
    let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    drop(listener);
    let _ = std::fs::remove_file(socket);
}

#[tokio::test(flavor = "current_thread")]
async fn secure_socket_rejects_regular_files_and_symlinks() {
    let directory = secure_runtime_subdir("forward-test-invalid").unwrap();
    let file = directory.join("file");
    let target = directory.join("target");
    let link = directory.join("link");
    for path in [&file, &target, &link] {
        let _ = std::fs::remove_file(path);
    }
    std::fs::write(&file, b"not a socket").unwrap();
    assert!(secure_socket(&file, &local_host()).await.is_err());
    std::fs::write(&target, b"target").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(secure_socket(&link, &local_host()).await.is_err());
}
