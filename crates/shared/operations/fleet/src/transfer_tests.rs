use soma_ops::Timestamp;

use super::*;

#[test]
fn transfer_contract_preserves_host_identity() {
    let request = TransferRequest::new(
        HostId::new("dookie").unwrap(),
        "/tmp/a",
        HostId::new("squirts").unwrap(),
        "/tmp/b",
        1024,
        Timestamp::from_unix_millis(100),
    )
    .unwrap();
    assert_eq!(request.source_host().as_str(), "dookie");
    assert_eq!(request.destination_host().as_str(), "squirts");
}

#[test]
fn transfer_receipt_exposes_verified_digests() {
    let digest = "a".repeat(64);
    let receipt = TransferReceipt::new(7)
        .with_digests(digest.clone(), digest.clone())
        .unwrap();
    assert_eq!(receipt.source_sha256(), Some(digest.as_str()));
    assert_eq!(receipt.destination_sha256(), Some(digest.as_str()));
    assert!(receipt.verified());
}
