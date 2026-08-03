use super::*;
use soma_ops::{OperationId, OperationName, ProgressEvent, ProgressSink, Timestamp};

struct FailingSink;

impl ProgressSink for FailingSink {
    type Error = &'static str;

    fn report(&self, _event: &ProgressEvent) -> Result<(), Self::Error> {
        Err("offline")
    }
}

#[test]
fn object_safe_progress_adapter_preserves_delivery_errors() {
    let event = ProgressEvent::new(
        OperationId::new(),
        OperationName::new("docker.pull").unwrap(),
        1,
        Timestamp::now(),
        "pull",
    )
    .unwrap();
    assert_eq!(
        MutationProgressReporter::report(&FailingSink, &event),
        Err("offline".into())
    );
}
