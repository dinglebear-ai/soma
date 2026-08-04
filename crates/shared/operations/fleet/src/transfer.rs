use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use soma_ops::Timestamp;

use crate::{HostId, RequestError, request::validate_absolute_path};

const MAX_TRANSFER_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

/// Descriptor-confined transfer request between managed hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRequest {
    source_host: HostId,
    source_path: PathBuf,
    destination_host: HostId,
    destination_path: PathBuf,
    max_bytes: u64,
    deadline: Timestamp,
}

impl TransferRequest {
    /// Creates a validated bounded transfer request.
    pub fn new(
        source_host: HostId,
        source_path: impl Into<PathBuf>,
        destination_host: HostId,
        destination_path: impl Into<PathBuf>,
        max_bytes: u64,
        deadline: Timestamp,
    ) -> Result<Self, RequestError> {
        if max_bytes == 0 || max_bytes > MAX_TRANSFER_BYTES {
            return Err(RequestError::InvalidTransferLimit {
                bytes: max_bytes,
                max: MAX_TRANSFER_BYTES,
            });
        }
        Ok(Self {
            source_host,
            source_path: validate_absolute_path(source_path.into())?,
            destination_host,
            destination_path: validate_absolute_path(destination_path.into())?,
            max_bytes,
            deadline,
        })
    }

    /// Rejects a request whose deadline has elapsed.
    pub fn validate_at(&self, now: Timestamp) -> Result<(), RequestError> {
        if self.deadline <= now {
            Err(RequestError::DeadlineElapsed)
        } else {
            Ok(())
        }
    }

    /// Returns the source host.
    #[must_use]
    pub fn source_host(&self) -> &HostId {
        &self.source_host
    }

    /// Returns the source absolute path.
    #[must_use]
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// Returns the destination host.
    #[must_use]
    pub fn destination_host(&self) -> &HostId {
        &self.destination_host
    }

    /// Returns the destination absolute path.
    #[must_use]
    pub fn destination_path(&self) -> &Path {
        &self.destination_path
    }

    /// Returns the maximum accepted byte count.
    #[must_use]
    pub const fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Returns the transfer deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Verified transfer completion metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferReceipt {
    bytes: u64,
    source_sha256: Option<String>,
    destination_sha256: Option<String>,
}

impl TransferReceipt {
    /// Creates a receipt without content digests.
    #[must_use]
    pub const fn new(bytes: u64) -> Self {
        Self {
            bytes,
            source_sha256: None,
            destination_sha256: None,
        }
    }

    /// Adds verified source and destination SHA-256 digests.
    pub fn with_digests(
        mut self,
        source: impl Into<String>,
        destination: impl Into<String>,
    ) -> Result<Self, RequestError> {
        let source = source.into();
        let destination = destination.into();
        validate_sha256(&source)?;
        validate_sha256(&destination)?;
        self.source_sha256 = Some(source);
        self.destination_sha256 = Some(destination);
        Ok(self)
    }

    /// Returns transferred bytes.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Returns the source SHA-256 when recorded.
    #[must_use]
    pub fn source_sha256(&self) -> Option<&str> {
        self.source_sha256.as_deref()
    }

    /// Returns the destination SHA-256 when recorded.
    #[must_use]
    pub fn destination_sha256(&self) -> Option<&str> {
        self.destination_sha256.as_deref()
    }

    /// Returns whether source and destination digests match.
    #[must_use]
    pub fn verified(&self) -> bool {
        matches!(
            (&self.source_sha256, &self.destination_sha256),
            (Some(source), Some(destination)) if source == destination
        )
    }
}

fn validate_sha256(value: &str) -> Result<(), RequestError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(RequestError::InvalidSha256)
    }
}
#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
