use std::os::unix::fs::symlink;

use soma_fleet::{HostEndpoint, HostId, SshEndpoint};

use super::*;

fn local_host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

#[tokio::test(flavor = "current_thread")]
async fn local_reader_stats_reads_and_hashes_bound_files() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("hello.txt");
    std::fs::write(&file, b"hello world").unwrap();
    let policy = FileReadPolicy::new([root.path()])
        .unwrap()
        .with_preview_limit(5)
        .unwrap()
        .with_hash_limit(1024)
        .unwrap();
    let inspector = LinuxFilesystemInspector::new(policy);

    let metadata = inspector
        .stat(&local_host(), &file, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(metadata.kind, FileKind::File);
    assert_eq!(metadata.size_bytes, 11);

    let preview = inspector
        .read(&local_host(), &file, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(preview.content, b"hello");
    assert!(preview.truncated);

    let hash = inspector
        .hash(&local_host(), &file, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        hash.sha256,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    assert_eq!(hash.bytes_hashed, 11);
}

#[tokio::test(flavor = "current_thread")]
async fn descriptor_binding_rejects_symlinks_and_outside_roots() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret");
    std::fs::write(&secret, b"secret").unwrap();
    let link = root.path().join("link");
    symlink(&secret, &link).unwrap();
    let inspector = LinuxFilesystemInspector::new(FileReadPolicy::new([root.path()]).unwrap());

    assert!(
        inspector
            .read(&local_host(), &link, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(matches!(
        inspector
            .stat(&local_host(), &secret, &CancellationToken::new())
            .await,
        Err(InfraError::PathOutsideRoots(_))
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn directories_hash_limits_remote_targets_and_cancellation_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("large");
    std::fs::write(&file, b"123456").unwrap();
    let inspector = LinuxFilesystemInspector::new(
        FileReadPolicy::new([root.path()])
            .unwrap()
            .with_hash_limit(5)
            .unwrap(),
    );
    assert!(
        inspector
            .read(&local_host(), root.path(), &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(
        inspector
            .hash(&local_host(), &file, &CancellationToken::new())
            .await
            .is_err()
    );

    let remote = HostRecord::new(
        HostId::new("remote").unwrap(),
        HostEndpoint::Ssh(SshEndpoint::new("remote").unwrap()),
    );
    assert!(matches!(
        inspector
            .stat(&remote, &file, &CancellationToken::new())
            .await,
        Err(InfraError::UnsupportedTarget { .. })
    ));

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
        inspector.stat(&local_host(), &file, &cancellation).await,
        Err(InfraError::Fleet(soma_fleet::FleetError::Cancelled))
    ));
}
