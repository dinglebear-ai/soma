use async_trait::async_trait;
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{FleetError, HostEndpoint, HostId, TransferRequest};

struct FixedClock(Timestamp);

impl FleetClock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

struct StaticRepository;

#[async_trait]
impl HostRepository for StaticRepository {
    async fn snapshot(&self) -> FleetResult<TopologySnapshot> {
        TopologySnapshot::new([HostRecord::new(
            HostId::new("dookie").unwrap(),
            HostEndpoint::Local,
        )])
        .map_err(FleetError::from)
    }
}

struct EchoExecutor;

#[async_trait]
impl CommandExecutor for EchoExecutor {
    async fn execute(
        &self,
        _host: &HostRecord,
        request: &CommandRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        if cancellation.is_cancelled() {
            return Err(FleetError::Cancelled);
        }
        Ok(CommandOutput::new(
            request.args().join(" ").into_bytes(),
            Vec::new(),
            Some(0),
            false,
        ))
    }
}

struct UnsupportedTransfer;

#[async_trait]
impl FileTransfer for UnsupportedTransfer {
    async fn transfer(
        &self,
        source: &HostRecord,
        destination: &HostRecord,
        _request: &TransferRequest,
        _cancellation: &CancellationToken,
    ) -> FleetResult<TransferReceipt> {
        Err(FleetError::Transfer {
            source_host: source.id().clone(),
            destination_host: destination.id().clone(),
            message: "unsupported".into(),
        })
    }
}

#[tokio::test]
async fn repository_executor_and_clock_ports_are_usable() {
    let snapshot = StaticRepository.snapshot().await.unwrap();
    let host = snapshot.hosts().next().unwrap();
    let request =
        CommandRequest::new("echo", ["hello", "fleet"], Timestamp::from_unix_millis(200)).unwrap();
    let output = EchoExecutor
        .execute(host, &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(output.stdout(), b"hello fleet");
    assert_eq!(
        FixedClock(Timestamp::from_unix_millis(100))
            .now()
            .unix_millis(),
        100
    );
}

#[tokio::test]
async fn cancellation_and_transfer_failures_are_typed() {
    let snapshot = StaticRepository.snapshot().await.unwrap();
    let host = snapshot.hosts().next().unwrap();
    let request = CommandRequest::new("echo", ["hello"], Timestamp::from_unix_millis(200)).unwrap();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        EchoExecutor.execute(host, &request, &cancellation).await,
        Err(FleetError::Cancelled)
    );

    let transfer = TransferRequest::new(
        host.id().clone(),
        "/tmp/source",
        host.id().clone(),
        "/tmp/destination",
        1024,
        Timestamp::from_unix_millis(200),
    )
    .unwrap();
    assert!(matches!(
        UnsupportedTransfer
            .transfer(host, host, &transfer, &CancellationToken::new())
            .await,
        Err(FleetError::Transfer { .. })
    ));
}
