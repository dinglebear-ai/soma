use std::sync::{Arc, Mutex, MutexGuard};

use crate::{FleetError, FleetResult, HostId, TransferReceipt, TransferRequest};

/// Observable terminal or in-flight transfer state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferGuardState {
    /// Transfer is active and has observed bounded bytes.
    InProgress {
        /// Bytes observed so far.
        bytes: u64,
        /// Maximum bytes admitted by the request.
        max_bytes: u64,
    },
    /// Transfer completed with optional digest verification.
    Completed {
        /// Completed byte count.
        bytes: u64,
        /// Whether source and destination digests matched.
        verified: bool,
    },
    /// Transfer was explicitly cancelled.
    Cancelled {
        /// Bytes observed before cancellation.
        bytes: u64,
    },
    /// Transfer failed with a bounded diagnostic.
    Failed {
        /// Bytes observed before failure.
        bytes: u64,
        /// Bounded failure detail.
        message: String,
    },
    /// Guard was dropped before a terminal method was called.
    Abandoned {
        /// Bytes observed before abandonment.
        bytes: u64,
    },
}

/// Cloneable observer for one transfer guard.
#[derive(Debug, Clone)]
pub struct TransferLifecycle {
    source: HostId,
    destination: HostId,
    state: Arc<Mutex<TransferGuardState>>,
}

impl TransferLifecycle {
    /// Starts a lifecycle and returns its mutable RAII guard.
    #[must_use]
    pub fn start(request: &TransferRequest) -> (Self, TransferGuard) {
        let state = Arc::new(Mutex::new(TransferGuardState::InProgress {
            bytes: 0,
            max_bytes: request.max_bytes(),
        }));
        let lifecycle = Self {
            source: request.source_host().clone(),
            destination: request.destination_host().clone(),
            state: Arc::clone(&state),
        };
        let guard = TransferGuard {
            state,
            terminal: false,
        };
        (lifecycle, guard)
    }

    /// Returns the source host.
    #[must_use]
    pub fn source(&self) -> &HostId {
        &self.source
    }
    /// Returns the destination host.
    #[must_use]
    pub fn destination(&self) -> &HostId {
        &self.destination
    }
    /// Returns a consistent state snapshot.
    #[must_use]
    pub fn snapshot(&self) -> TransferGuardState {
        lock(&self.state).clone()
    }
}

/// RAII transfer accounting guard.
pub struct TransferGuard {
    state: Arc<Mutex<TransferGuardState>>,
    terminal: bool,
}

impl TransferGuard {
    /// Records a completed chunk and rejects overflow or bound violations.
    pub fn record_chunk(&mut self, bytes: u64) -> FleetResult<u64> {
        let mut state = lock(&self.state);
        let TransferGuardState::InProgress {
            bytes: observed,
            max_bytes,
        } = &mut *state
        else {
            return Err(FleetError::TransferLifecycle(
                "cannot record bytes after a terminal state".into(),
            ));
        };
        let next = observed.checked_add(bytes).ok_or_else(|| {
            FleetError::TransferLifecycle("transfer byte counter overflow".into())
        })?;
        if next > *max_bytes {
            return Err(FleetError::TransferLifecycle(format!(
                "transfer exceeded maximum of {max_bytes} bytes"
            )));
        }
        *observed = next;
        Ok(next)
    }

    /// Marks the transfer complete after checking receipt byte parity.
    pub fn complete(mut self, receipt: TransferReceipt) -> FleetResult<TransferReceipt> {
        let observed = match &*lock(&self.state) {
            TransferGuardState::InProgress { bytes, .. } => *bytes,
            _ => {
                return Err(FleetError::TransferLifecycle(
                    "cannot complete a terminal transfer".into(),
                ));
            }
        };
        if observed != receipt.bytes() {
            return Err(FleetError::TransferLifecycle(format!(
                "receipt reports {} bytes but lifecycle observed {observed}",
                receipt.bytes()
            )));
        }
        *lock(&self.state) = TransferGuardState::Completed {
            bytes: observed,
            verified: receipt.verified(),
        };
        self.terminal = true;
        Ok(receipt)
    }

    /// Marks the transfer cancelled.
    pub fn cancel(mut self) -> FleetResult<()> {
        let bytes = in_progress_bytes(&self.state)?;
        *lock(&self.state) = TransferGuardState::Cancelled { bytes };
        self.terminal = true;
        Ok(())
    }

    /// Marks the transfer failed with bounded detail.
    pub fn fail(mut self, message: impl Into<String>) -> FleetResult<()> {
        let message = message.into();
        if message.is_empty()
            || message.chars().count() > 1024
            || message.chars().any(char::is_control)
        {
            return Err(FleetError::TransferLifecycle(
                "transfer failure detail is invalid".into(),
            ));
        }
        let bytes = in_progress_bytes(&self.state)?;
        *lock(&self.state) = TransferGuardState::Failed { bytes, message };
        self.terminal = true;
        Ok(())
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }
        let mut state = lock(&self.state);
        if let TransferGuardState::InProgress { bytes, .. } = *state {
            *state = TransferGuardState::Abandoned { bytes };
        }
    }
}

fn in_progress_bytes(state: &Mutex<TransferGuardState>) -> FleetResult<u64> {
    match &*lock(state) {
        TransferGuardState::InProgress { bytes, .. } => Ok(*bytes),
        _ => Err(FleetError::TransferLifecycle(
            "transfer is already terminal".into(),
        )),
    }
}

fn lock(state: &Mutex<TransferGuardState>) -> MutexGuard<'_, TransferGuardState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
#[path = "transfer_guard_tests.rs"]
mod tests;
