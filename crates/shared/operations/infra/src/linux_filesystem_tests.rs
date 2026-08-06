use std::io::{Cursor, Read};
use std::os::unix::fs::symlink;
use std::process::Command;
use std::time::Duration;

use soma_fleet::{HostEndpoint, HostId, SshEndpoint};

use super::*;

fn local_host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
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

#[tokio::test(flavor = "current_thread")]
async fn special_files_fail_promptly_without_blocking_the_runtime() {
    let root = tempfile::tempdir().unwrap();
    let fifo = root.path().join("pipe");
    let status = Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(status.success());
    let inspector = LinuxFilesystemInspector::new(FileReadPolicy::new([root.path()]).unwrap());

    let result = tokio::time::timeout(
        Duration::from_secs(1),
        inspector.read(&local_host(), &fifo, &CancellationToken::new()),
    )
    .await
    .expect("FIFO inspection must not block");
    assert!(matches!(result, Err(InfraError::Filesystem { .. })));

    let metadata = tokio::time::timeout(
        Duration::from_secs(1),
        inspector.stat(&local_host(), &fifo, &CancellationToken::new()),
    )
    .await
    .expect("FIFO stat must not block");
    assert!(matches!(metadata, Err(InfraError::Filesystem { .. })));
}

#[tokio::test(flavor = "current_thread")]
async fn hash_observes_cancellation_before_reading() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("data");
    std::fs::write(&file, vec![0_u8; 1024 * 1024]).unwrap();
    let inspector = LinuxFilesystemInspector::new(FileReadPolicy::new([root.path()]).unwrap());
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    assert!(matches!(
        inspector.hash(&local_host(), &file, &cancellation).await,
        Err(InfraError::Fleet(soma_fleet::FleetError::Cancelled))
    ));
}

#[test]
fn hash_reader_stops_at_the_byte_ceiling_even_if_the_source_is_larger() {
    let mut reader = Cursor::new(b"123456".to_vec());
    let result = hash_reader(
        &mut reader,
        Path::new("growing-file"),
        5,
        &CancellationToken::new(),
    );
    assert!(matches!(result, Err(InfraError::InvalidRequest { .. })));
    assert_eq!(reader.position(), 6);
}

#[test]
fn hash_reader_observes_cancellation_between_chunks() {
    struct CancelAfterFirstRead {
        source: Cursor<Vec<u8>>,
        cancellation: CancellationToken,
    }

    impl Read for CancelAfterFirstRead {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let read = self.source.read(buffer)?;
            self.cancellation.cancel();
            Ok(read)
        }
    }

    let cancellation = CancellationToken::new();
    let mut reader = CancelAfterFirstRead {
        source: Cursor::new(vec![0_u8; 128 * 1024]),
        cancellation: cancellation.clone(),
    };
    let result = hash_reader(
        &mut reader,
        Path::new("large-file"),
        128 * 1024,
        &cancellation,
    );
    assert!(matches!(
        result,
        Err(InfraError::Fleet(soma_fleet::FleetError::Cancelled))
    ));
    assert_eq!(reader.source.position(), 64 * 1024);
}
