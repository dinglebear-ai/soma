use std::path::Path;

use soma_fleet::{HostRecord, TransferRequest};
use soma_ops::MutationSendState;
use tokio_util::sync::CancellationToken;

use crate::file_transfer::receipt_identity;
use crate::{
    FileTransferFingerprint, FileTransferPathRole, InfraError, InfraResult,
    MAX_FILE_TRANSFER_BYTES, MutationFailure, MutationResult, VerifiedFileTransferClient,
    VerifiedFileTransferOutcome, VerifiedFileTransferRequest,
};

/// Verified bounded file-transfer coordinator.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileTransferEngine;

impl FileTransferEngine {
    /// Captures source and destination pre-state.
    pub async fn inspect(
        &self,
        client: &dyn VerifiedFileTransferClient,
        source: &HostRecord,
        source_path: &Path,
        destination: &HostRecord,
        destination_path: &Path,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileTransferFingerprint> {
        let source_identity = client
            .inspect_transfer_file(
                source,
                source_path,
                FileTransferPathRole::Source,
                false,
                cancellation,
            )
            .await?
            .ok_or_else(|| InfraError::InvalidRequest {
                domain: "file-transfer",
                message: "source file is absent".into(),
            })?;
        if source_identity.bytes > MAX_FILE_TRANSFER_BYTES {
            return Err(InfraError::InvalidRequest {
                domain: "file-transfer",
                message: format!(
                    "source exceeds the {MAX_FILE_TRANSFER_BYTES}-byte transfer limit"
                ),
            });
        }
        let destination_before = client
            .inspect_transfer_file(
                destination,
                destination_path,
                FileTransferPathRole::Destination,
                true,
                cancellation,
            )
            .await?;
        Ok(FileTransferFingerprint {
            source_host: source.id().clone(),
            source_revision: source.revision().clone(),
            source: source_identity,
            destination_host: destination.id().clone(),
            destination_revision: destination.revision().clone(),
            destination_path: destination_path.to_path_buf(),
            destination_before,
        })
    }

    /// Executes a transfer and independently verifies destination content.
    pub async fn execute(
        &self,
        client: &dyn VerifiedFileTransferClient,
        source: &HostRecord,
        destination: &HostRecord,
        request: &VerifiedFileTransferRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<VerifiedFileTransferOutcome> {
        if cancellation.is_cancelled() {
            return Err(not_sent(soma_fleet::FleetError::Cancelled.into()));
        }
        if request.deadline <= soma_ops::Timestamp::now() {
            return Err(not_sent(soma_fleet::FleetError::DeadlineExceeded.into()));
        }
        let current = self
            .inspect(
                client,
                source,
                &request.fingerprint.source.path,
                destination,
                &request.fingerprint.destination_path,
                cancellation,
            )
            .await
            .map_err(not_sent)?;
        if current != request.fingerprint {
            return Err(not_sent(InfraError::InvalidRequest {
                domain: "file-transfer",
                message: "source or destination state changed after planning".into(),
            }));
        }
        let transfer = TransferRequest::new(
            source.id().clone(),
            request.fingerprint.source.path.clone(),
            destination.id().clone(),
            request.fingerprint.destination_path.clone(),
            MAX_FILE_TRANSFER_BYTES,
            request.deadline,
        )
        .map_err(soma_fleet::FleetError::from)
        .map_err(|error| not_sent(error.into()))?
        .with_expected_source_sha256(request.fingerprint.source.sha256.clone());
        let receipt = client
            .transfer(source, destination, &transfer, cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::Unknown, error.into()))?;
        let destination_after = client
            .inspect_transfer_file(
                destination,
                &request.fingerprint.destination_path,
                FileTransferPathRole::Destination,
                false,
                cancellation,
            )
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::Sent, error))?
            .ok_or_else(|| {
                MutationFailure::new(
                    MutationSendState::Sent,
                    InfraError::InvalidRequest {
                        domain: "file-transfer",
                        message: "destination is absent after transfer".into(),
                    },
                )
            })?;
        let (source_digest, destination_digest) = receipt_identity(&receipt)
            .map_err(|error| MutationFailure::new(MutationSendState::Sent, error))?;
        let verified = receipt.verified()
            && receipt.bytes() == request.fingerprint.source.bytes
            && destination_after.bytes == request.fingerprint.source.bytes
            && source_digest == request.fingerprint.source.sha256
            && destination_digest == destination_after.sha256
            && source_digest == destination_digest;
        if !verified {
            return Err(MutationFailure::new(
                MutationSendState::Sent,
                InfraError::InvalidRequest {
                    domain: "file-transfer",
                    message: "source and destination transfer evidence does not match".into(),
                },
            ));
        }
        let changed = request
            .fingerprint
            .destination_before
            .as_ref()
            .is_none_or(|before| before.sha256 != destination_after.sha256);
        Ok(VerifiedFileTransferOutcome {
            before: request.fingerprint.clone(),
            destination_after,
            bytes: receipt.bytes(),
            send_state: MutationSendState::Sent,
            verified,
            changed,
        })
    }
}

fn not_sent(error: InfraError) -> MutationFailure {
    MutationFailure::new(MutationSendState::NotSent, error)
}

#[cfg(test)]
#[path = "file_transfer_engine_tests.rs"]
mod tests;
