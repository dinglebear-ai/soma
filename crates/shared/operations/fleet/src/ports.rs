use async_trait::async_trait;
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{
    CommandOutput, CommandRequest, FleetResult, HostRecord, TopologySnapshot, TransferReceipt,
    TransferRequest,
};

/// Source of immutable fleet topology snapshots.
#[async_trait]
pub trait HostRepository: Send + Sync {
    /// Loads one internally consistent topology snapshot.
    async fn snapshot(&self) -> FleetResult<TopologySnapshot>;
}

/// Driver that opens and closes revision-bound host connections.
#[async_trait]
pub trait ConnectionFactory: Send + Sync {
    /// Concrete connection handle cached by consumers.
    type Connection: Send + Sync + 'static;

    /// Opens a connection for the exact host revision.
    async fn connect(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> FleetResult<Self::Connection>;

    /// Closes a connection explicitly when invalidated or evicted.
    async fn close(&self, connection: &Self::Connection) -> FleetResult<()>;
}

/// Exec-style command driver for local, SSH, or other host transports.
#[async_trait]
pub trait CommandExecutor: Send + Sync {
    /// Executes a validated command request on one exact host revision.
    async fn execute(
        &self,
        host: &HostRecord,
        request: &CommandRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput>;
}

/// Descriptor-confined file transfer driver.
#[async_trait]
pub trait FileTransfer: Send + Sync {
    /// Transfers one validated path pair between exact host revisions.
    async fn transfer(
        &self,
        source: &HostRecord,
        destination: &HostRecord,
        request: &TransferRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<TransferReceipt>;
}

/// Clock used for request deadline admission and deterministic tests.
pub trait FleetClock: Send + Sync {
    /// Returns current Unix-millisecond time.
    fn now(&self) -> Timestamp;
}

/// System-wall-clock implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemFleetClock;

impl FleetClock for SystemFleetClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[cfg(test)]
#[path = "ports_tests.rs"]
mod tests;
