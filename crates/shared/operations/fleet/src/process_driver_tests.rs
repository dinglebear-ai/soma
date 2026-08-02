use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::{HostId, HostRecord};

fn local() -> HostRecord {
    HostRecord::new(HostId::new("local").unwrap(), HostEndpoint::Local)
}

fn deadline_after(duration: Duration) -> soma_ops::Timestamp {
    soma_ops::Timestamp::from_unix_millis(
        soma_ops::Timestamp::now().unix_millis() + duration.as_millis() as i64,
    )
}

#[tokio::test(flavor = "current_thread")]
async fn process_driver_preserves_argument_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("should-not-exist");
    let argument = format!("hello; touch {}", marker.display());
    let request = CommandRequest::new(
        "/usr/bin/printf",
        ["%s", argument.as_str()],
        deadline_after(Duration::from_secs(2)),
    )
    .unwrap();
    let output = LocalProcessDriver
        .execute(&local(), &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(output.stdout(), argument.as_bytes());
    assert!(!marker.exists());
    assert_eq!(output.exit_code(), Some(0));
}

#[tokio::test(flavor = "current_thread")]
async fn process_driver_truncates_output_without_stopping_drain() {
    let request = CommandRequest::new(
        "/usr/bin/head",
        ["-c", "4096", "/dev/zero"],
        deadline_after(Duration::from_secs(2)),
    )
    .unwrap()
    .with_output_limits(128, 128)
    .unwrap();
    let output = LocalProcessDriver
        .execute(&local(), &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(output.stdout().len(), 128);
    assert!(output.truncated());
}

#[tokio::test(flavor = "current_thread")]
async fn process_driver_distinguishes_pre_cancel_and_inflight_timeout() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let request = CommandRequest::new(
        "/usr/bin/sleep",
        ["1"],
        deadline_after(Duration::from_secs(2)),
    )
    .unwrap();
    assert_eq!(
        LocalProcessDriver
            .execute(&local(), &request, &cancellation)
            .await,
        Err(FleetError::Cancelled)
    );

    let request = CommandRequest::new(
        "/usr/bin/sleep",
        ["1"],
        deadline_after(Duration::from_millis(25)),
    )
    .unwrap();
    assert_eq!(
        LocalProcessDriver
            .execute(&local(), &request, &CancellationToken::new())
            .await,
        Err(FleetError::DeadlineExceeded)
    );
}
