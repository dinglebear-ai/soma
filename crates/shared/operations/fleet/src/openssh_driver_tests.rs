use tokio_util::sync::CancellationToken;

use super::*;
use crate::SshEndpoint;

fn ssh_host() -> HostRecord {
    HostRecord::new(
        HostId::new("devhost").unwrap(),
        HostEndpoint::Ssh(
            SshEndpoint::new("198.51.100.10")
                .unwrap()
                .with_user("devuser")
                .unwrap(),
        ),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn driver_rejects_non_ssh_working_directory_and_pre_cancel_without_network() {
    let driver = OpenSshDriver::default();
    let local = HostRecord::new(HostId::new("local").unwrap(), HostEndpoint::Local);
    let request = CommandRequest::new(
        "hostname",
        Vec::<String>::new(),
        soma_ops::Timestamp::from_unix_millis(soma_ops::Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap();
    assert!(matches!(
        driver
            .execute(&local, &request, &CancellationToken::new())
            .await,
        Err(FleetError::Command { .. })
    ));

    let request = request.with_working_dir("/tmp").unwrap();
    assert!(matches!(
        driver
            .execute(&ssh_host(), &request, &CancellationToken::new())
            .await,
        Err(FleetError::Command { .. })
    ));

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let request = CommandRequest::new(
        "hostname",
        Vec::<String>::new(),
        soma_ops::Timestamp::from_unix_millis(soma_ops::Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap();
    assert_eq!(
        driver.execute(&ssh_host(), &request, &cancellation).await,
        Err(FleetError::Cancelled)
    );
}
