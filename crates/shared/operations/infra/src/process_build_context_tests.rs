use std::path::Path;
use std::sync::Arc;

use soma_fleet::{HostEndpoint, HostId, HostRecord, LocalProcessDriver};
use tempfile::tempdir;

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("local").unwrap(), HostEndpoint::Local)
}
fn deadline() -> soma_ops::Timestamp {
    soma_ops::Timestamp::from_unix_millis(soma_ops::Timestamp::now().unix_millis() + 10_000)
}

#[tokio::test]
async fn context_fingerprint_is_deterministic_and_content_sensitive() {
    let root = tempdir().unwrap();
    std::fs::create_dir(root.path().join("app")).unwrap();
    std::fs::write(
        root.path().join("app/Dockerfile"),
        b"FROM scratch
",
    )
    .unwrap();
    std::fs::write(root.path().join("app/data.txt"), b"one").unwrap();
    let inspector = CommandBuildContextInspector::new(
        Arc::new(LocalProcessDriver),
        BuildContextPolicy::new([root.path()]).unwrap(),
    );
    let first = inspector
        .fingerprint(
            &host(),
            &root.path().join("app"),
            deadline(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let second = inspector
        .fingerprint(
            &host(),
            &root.path().join("app"),
            deadline(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.file_count, 2);
    std::fs::write(root.path().join("app/data.txt"), b"two").unwrap();
    let changed = inspector
        .fingerprint(
            &host(),
            &root.path().join("app"),
            deadline(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_ne!(first.sha256, changed.sha256);
}

#[cfg(unix)]
#[tokio::test]
async fn context_fingerprint_rejects_symlinks_and_outside_roots() {
    use std::os::unix::fs::symlink;
    let root = tempdir().unwrap();
    std::fs::create_dir(root.path().join("app")).unwrap();
    symlink("/etc/passwd", root.path().join("app/link")).unwrap();
    let inspector = CommandBuildContextInspector::new(
        Arc::new(LocalProcessDriver),
        BuildContextPolicy::new([root.path()]).unwrap(),
    );
    assert!(
        inspector
            .fingerprint(
                &host(),
                &root.path().join("app"),
                deadline(),
                &CancellationToken::new()
            )
            .await
            .is_err()
    );
    assert!(
        inspector
            .fingerprint(
                &host(),
                Path::new("/etc"),
                deadline(),
                &CancellationToken::new()
            )
            .await
            .is_err()
    );
}
