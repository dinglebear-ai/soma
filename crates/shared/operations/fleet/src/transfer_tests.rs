use soma_ops::Timestamp;

use super::*;

#[test]
fn transfer_contract_preserves_host_identity() {
    let request = TransferRequest::new(
        HostId::new("devhost").unwrap(),
        "/tmp/a",
        HostId::new("edgehost").unwrap(),
        "/tmp/b",
        1024,
        Timestamp::from_unix_millis(100),
    )
    .unwrap();
    assert_eq!(request.source_host().as_str(), "devhost");
    assert_eq!(request.destination_host().as_str(), "edgehost");
}
