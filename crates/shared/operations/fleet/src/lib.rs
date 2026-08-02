//! Neutral host topology, connection, transfer, and bounded fanout contracts.
//!
//! `soma-fleet` defines transport-independent identities, topology snapshots,
//! request bounds, execution ports, revision-keyed connection caching, and a
//! cancellation-aware fanout scheduler. Product configuration, authorization,
//! command allowlists, and concrete SSH clients live above or behind this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod cache;
mod command;
mod endpoint;
mod error;
mod event;
mod fanout;
#[cfg(all(feature = "openssh-driver", unix))]
mod forward;
mod identity;
#[cfg(any(feature = "openssh-driver", feature = "process-driver"))]
mod io;
#[cfg(all(feature = "openssh-driver", unix))]
mod openssh_connector;
#[cfg(all(feature = "openssh-driver", unix))]
mod openssh_driver;
mod pool;
mod ports;
#[cfg(feature = "process-driver")]
mod process_driver;
mod request;
#[cfg(all(feature = "openssh-driver", unix))]
mod runtime;
mod topology;
mod transfer;
mod transfer_guard;

pub use cache::ConnectionCache;
pub use command::{CommandOutput, CommandRequest};
pub use endpoint::{HostEndpoint, HttpEndpoint, SshEndpoint};
pub use error::{FleetError, FleetResult};
pub use event::{FleetEvent, FleetEventKind, FleetEventSink, NoopFleetEventSink};
pub use fanout::{
    FanoutPolicy, FanoutPolicyError, FanoutReport, FanoutScheduler, TargetOutcome,
    TargetOutcomeKind,
};
#[cfg(all(feature = "openssh-driver", unix))]
pub use forward::{ForwardedUnixSocket, forwarded_socket_path};
pub use identity::{CapabilityName, HostId, IdentityError, PoolKey, TopologyRevision};
#[cfg(all(feature = "openssh-driver", unix))]
pub use openssh_connector::{OpenSshConnectPlan, OpenSshConnection, OpenSshConnector};
#[cfg(all(feature = "openssh-driver", unix))]
pub use openssh_driver::OpenSshDriver;
pub use pool::ConnectionPool;
pub use ports::{
    CommandExecutor, ConnectionFactory, FileTransfer, FleetClock, HostRepository, SystemFleetClock,
};
#[cfg(feature = "process-driver")]
pub use process_driver::LocalProcessDriver;
pub use request::RequestError;
pub use topology::{HostRecord, TopologyError, TopologySnapshot};
pub use transfer::{TransferReceipt, TransferRequest};
pub use transfer_guard::{TransferGuard, TransferGuardState, TransferLifecycle};
