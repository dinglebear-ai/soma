use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_ops::Timestamp;

use crate::{FleetResult, HostId, TopologyRevision};

/// Stable fleet lifecycle event category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetEventKind {
    /// A topology snapshot was loaded.
    TopologyLoaded,
    /// A connection was opened for one exact revision.
    ConnectionOpened,
    /// A cached connection was invalidated.
    ConnectionInvalidated,
    /// Command execution began.
    CommandStarted,
    /// Command execution completed.
    CommandCompleted,
    /// Transfer began.
    TransferStarted,
    /// Transfer completed.
    TransferCompleted,
    /// One target was cancelled.
    Cancelled,
    /// One target exceeded its deadline.
    TimedOut,
}

/// Product-neutral fleet lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetEvent {
    kind: FleetEventKind,
    host: Option<HostId>,
    revision: Option<TopologyRevision>,
    occurred_at: Timestamp,
    detail: Option<String>,
}

impl FleetEvent {
    /// Creates a fleet event.
    #[must_use]
    pub const fn new(kind: FleetEventKind, occurred_at: Timestamp) -> Self {
        Self {
            kind,
            host: None,
            revision: None,
            occurred_at,
            detail: None,
        }
    }

    /// Binds the event to one host and topology revision.
    #[must_use]
    pub fn with_host(mut self, host: &crate::HostRecord) -> Self {
        self.host = Some(host.id().clone());
        self.revision = Some(host.revision().clone());
        self
    }

    /// Adds bounded human-readable detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> FleetResult<Self> {
        let detail = detail.into();
        let count = detail.chars().count();
        if count == 0 || count > 1024 || detail.chars().any(char::is_control) {
            return Err(crate::FleetError::EventSink(
                "invalid fleet event detail".into(),
            ));
        }
        self.detail = Some(detail);
        Ok(self)
    }

    /// Returns the event kind.
    #[must_use]
    pub const fn kind(&self) -> FleetEventKind {
        self.kind
    }

    /// Returns the optional host identity.
    #[must_use]
    pub fn host(&self) -> Option<&HostId> {
        self.host.as_ref()
    }

    /// Returns the optional topology revision.
    #[must_use]
    pub fn revision(&self) -> Option<&TopologyRevision> {
        self.revision.as_ref()
    }

    /// Returns event time.
    #[must_use]
    pub const fn occurred_at(&self) -> Timestamp {
        self.occurred_at
    }

    /// Returns optional event detail.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

/// Sink for durable or observable fleet lifecycle events.
#[async_trait]
pub trait FleetEventSink: Send + Sync {
    /// Emits one event.
    async fn emit(&self, event: FleetEvent) -> FleetResult<()>;
}

/// Event sink that discards all events.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopFleetEventSink;

#[async_trait]
impl FleetEventSink for NoopFleetEventSink {
    async fn emit(&self, _event: FleetEvent) -> FleetResult<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
