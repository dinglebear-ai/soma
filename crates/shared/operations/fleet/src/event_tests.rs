use soma_ops::Timestamp;

use super::*;
use crate::{HostEndpoint, HostRecord};

#[tokio::test(flavor = "current_thread")]
async fn events_bind_host_revision_and_emit() {
    let host = HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local);
    let event = FleetEvent::new(
        FleetEventKind::ConnectionOpened,
        Timestamp::from_unix_millis(100),
    )
    .with_host(&host)
    .with_detail("local connection ready")
    .unwrap();
    assert_eq!(event.kind(), FleetEventKind::ConnectionOpened);
    assert_eq!(event.host(), Some(host.id()));
    assert_eq!(event.revision(), Some(host.revision()));
    assert_eq!(event.occurred_at().unix_millis(), 100);
    assert_eq!(event.detail(), Some("local connection ready"));
    NoopFleetEventSink.emit(event).await.unwrap();
}

#[test]
fn event_details_are_bounded() {
    assert!(
        FleetEvent::new(
            FleetEventKind::TopologyLoaded,
            Timestamp::from_unix_millis(0)
        )
        .with_detail("")
        .is_err()
    );
    assert!(
        FleetEvent::new(
            FleetEventKind::TopologyLoaded,
            Timestamp::from_unix_millis(0)
        )
        .with_detail("x".repeat(1025))
        .is_err()
    );
}
