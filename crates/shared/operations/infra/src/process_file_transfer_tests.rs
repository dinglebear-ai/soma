use std::fs;
use std::os::unix::fs::symlink;
use std::sync::Arc;

use soma_fleet::{
    FileTransfer, HostEndpoint, HostId, HostRecord, LocalProcessDriver, TransferRequest,
};
use soma_ops::Timestamp;

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

#[tokio::test]
async fn local_transfer_copies_and_verifies_bytes() {
    let source_root = tempfile::tempdir().unwrap();
    let destination_root = tempfile::tempdir().unwrap();
    let source = source_root.path().join("source.txt");
    let destination = destination_root.path().join("destination.txt");
    fs::write(&source, b"soma transfer").unwrap();
    let host = host();
    let driver = CommandFileTransfer::new(Arc::new(LocalProcessDriver)).with_policy(
        host.id().clone(),
        FileTransferPolicy::new([source_root.path()], [destination_root.path()]).unwrap(),
    );
    let request = TransferRequest::new(
        host.id().clone(),
        source.clone(),
        host.id().clone(),
        destination.clone(),
        crate::MAX_FILE_TRANSFER_BYTES,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 20_000),
    )
    .unwrap();
    let receipt = driver
        .transfer(&host, &host, &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(receipt.bytes(), 13);
    assert!(receipt.verified());
    assert_eq!(fs::read(destination).unwrap(), b"soma transfer");
}

#[tokio::test]
async fn destination_symlink_escape_is_rejected() {
    let source_root = tempfile::tempdir().unwrap();
    let destination_root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = source_root.path().join("source.txt");
    let destination = destination_root.path().join("destination.txt");
    let secret = outside.path().join("secret.txt");
    fs::write(&source, b"safe").unwrap();
    fs::write(&secret, b"secret").unwrap();
    symlink(&secret, &destination).unwrap();
    let host = host();
    let driver = CommandFileTransfer::new(Arc::new(LocalProcessDriver)).with_policy(
        host.id().clone(),
        FileTransferPolicy::new([source_root.path()], [destination_root.path()]).unwrap(),
    );
    let request = TransferRequest::new(
        host.id().clone(),
        source,
        host.id().clone(),
        destination,
        crate::MAX_FILE_TRANSFER_BYTES,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 20_000),
    )
    .unwrap();
    assert!(
        driver
            .transfer(&host, &host, &request, &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(fs::read(secret).unwrap(), b"secret");
}
