use soma_ops::Timestamp;

use super::*;

fn request(max_bytes: u64) -> TransferRequest {
    TransferRequest::new(
        HostId::new("devhost").unwrap(),
        "/tmp/source",
        HostId::new("edgehost").unwrap(),
        "/tmp/destination",
        max_bytes,
        Timestamp::from_unix_millis(100),
    )
    .unwrap()
}

#[test]
fn guard_records_completion_and_digest_verification() {
    let (lifecycle, mut guard) = TransferLifecycle::start(&request(10));
    guard.record_chunk(4).unwrap();
    guard.record_chunk(6).unwrap();
    let receipt = TransferReceipt::new(10)
        .with_digests("a".repeat(64), "a".repeat(64))
        .unwrap();
    guard.complete(receipt).unwrap();
    assert_eq!(
        lifecycle.snapshot(),
        TransferGuardState::Completed {
            bytes: 10,
            verified: true
        }
    );
    assert_eq!(lifecycle.source().as_str(), "devhost");
    assert_eq!(lifecycle.destination().as_str(), "edgehost");
}

#[test]
fn guard_rejects_overrun_and_receipt_mismatch() {
    let (_lifecycle, mut guard) = TransferLifecycle::start(&request(5));
    assert!(guard.record_chunk(6).is_err());

    let (_lifecycle, mut guard) = TransferLifecycle::start(&request(5));
    guard.record_chunk(4).unwrap();
    assert!(guard.complete(TransferReceipt::new(5)).is_err());
}

#[test]
fn terminal_and_drop_states_remain_observable() {
    let (cancelled, mut guard) = TransferLifecycle::start(&request(5));
    guard.record_chunk(2).unwrap();
    guard.cancel().unwrap();
    assert_eq!(
        cancelled.snapshot(),
        TransferGuardState::Cancelled { bytes: 2 }
    );

    let (failed, mut guard) = TransferLifecycle::start(&request(5));
    guard.record_chunk(3).unwrap();
    guard.fail("remote write failed").unwrap();
    assert!(matches!(
        failed.snapshot(),
        TransferGuardState::Failed { bytes: 3, .. }
    ));

    let (abandoned, mut guard) = TransferLifecycle::start(&request(5));
    guard.record_chunk(1).unwrap();
    drop(guard);
    assert_eq!(
        abandoned.snapshot(),
        TransferGuardState::Abandoned { bytes: 1 }
    );
}
