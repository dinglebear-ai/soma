use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use soma_fleet::{FanoutPolicy, FanoutScheduler, HostId, HostRecord, TargetOutcomeKind};
use soma_ops::MutationSendState;
use tokio_util::sync::CancellationToken;

use crate::{
    HostExecMutator, HostExecReceipt, HostExecRequest, InfraError, MutationFailure, MutationResult,
};

/// Terminal classification for one host-exec fanout target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostExecTargetStatus {
    /// Command completed with exit code zero.
    Succeeded,
    /// Command failed, returned nonzero, or lost backend certainty.
    Failed,
    /// Shared cancellation interrupted target accounting.
    Cancelled,
    /// Fanout target exceeded its per-target ceiling.
    TimedOut,
}

/// Stable-order outcome for one fanout target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostExecTargetResult {
    /// Target host.
    pub host: HostId,
    /// Optional descriptor-bound working directory.
    pub working_dir: Option<PathBuf>,
    /// Terminal target status.
    pub status: HostExecTargetStatus,
    /// Completed command receipt when available.
    pub receipt: Option<HostExecReceipt>,
    /// Bounded error text when execution did not succeed.
    pub error: Option<String>,
    /// Conservative backend send state.
    pub send_state: MutationSendState,
}

/// Complete stable-order host execution fanout outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostExecManyOutcome {
    /// Per-target results in normalized request order.
    pub results: Vec<HostExecTargetResult>,
    /// Successful target count.
    pub succeeded: usize,
    /// Failed target count, including nonzero exits.
    pub failed: usize,
    /// Cancelled target count.
    pub cancelled: usize,
    /// Timed-out target count.
    pub timed_out: usize,
    /// Aggregate conservative send state.
    pub send_state: MutationSendState,
}

impl HostExecManyOutcome {
    /// Returns whether every target completed with exit code zero.
    #[must_use]
    pub fn all_succeeded(&self) -> bool {
        self.succeeded == self.results.len()
    }
}

/// Bounded stable-order host execution fanout coordinator.
#[derive(Debug, Clone, Copy)]
pub struct HostExecManyEngine {
    scheduler: FanoutScheduler,
}

impl HostExecManyEngine {
    /// Creates an engine with explicit concurrency and per-target timeout bounds.
    pub fn new(max_concurrency: usize, per_target_timeout: Duration) -> Result<Self, InfraError> {
        let policy = FanoutPolicy::new(max_concurrency, per_target_timeout).map_err(|error| {
            InfraError::InvalidRequest {
                domain: "host-exec-many",
                message: error.to_string(),
            }
        })?;
        Ok(Self {
            scheduler: FanoutScheduler::new(policy),
        })
    }

    /// Executes distinct host/request payloads and retains every terminal outcome.
    pub async fn execute(
        &self,
        client: &dyn HostExecMutator,
        targets: Vec<(HostRecord, HostExecRequest)>,
        cancellation: CancellationToken,
    ) -> MutationResult<HostExecManyOutcome> {
        if targets.is_empty() {
            return Err(MutationFailure::new(
                MutationSendState::NotSent,
                InfraError::InvalidRequest {
                    domain: "host-exec-many",
                    message: "at least one target is required".into(),
                },
            ));
        }
        if cancellation.is_cancelled() {
            return Err(MutationFailure::new(
                MutationSendState::NotSent,
                soma_fleet::FleetError::Cancelled.into(),
            ));
        }
        let descriptors = targets
            .iter()
            .map(|(host, request)| {
                (
                    host.id().clone(),
                    request.working_dir().map(ToOwned::to_owned),
                )
            })
            .collect::<Vec<_>>();
        let report = self
            .scheduler
            .run_with_payload(targets, cancellation, |host, request, child| async move {
                client.exec_host(&host, &request, &child).await
            })
            .await;
        let mut results = Vec::with_capacity(descriptors.len());
        for outcome in report.into_outcomes() {
            let (index, host, kind) = outcome.into_parts();
            let working_dir = descriptors[index].1.clone();
            results.push(normalize_target(host, working_dir, kind));
        }
        let succeeded = count(&results, HostExecTargetStatus::Succeeded);
        let failed = count(&results, HostExecTargetStatus::Failed);
        let cancelled = count(&results, HostExecTargetStatus::Cancelled);
        let timed_out = count(&results, HostExecTargetStatus::TimedOut);
        let send_state = aggregate_send_state(&results);
        Ok(HostExecManyOutcome {
            results,
            succeeded,
            failed,
            cancelled,
            timed_out,
            send_state,
        })
    }
}

fn normalize_target(
    host: HostId,
    working_dir: Option<PathBuf>,
    kind: TargetOutcomeKind<HostExecReceipt, MutationFailure>,
) -> HostExecTargetResult {
    match kind {
        TargetOutcomeKind::Succeeded(receipt) => {
            let succeeded = receipt.exit_code == Some(0);
            let send_state = receipt.send_state;
            HostExecTargetResult {
                host,
                working_dir,
                status: if succeeded {
                    HostExecTargetStatus::Succeeded
                } else {
                    HostExecTargetStatus::Failed
                },
                error: if succeeded {
                    None
                } else {
                    Some(format!(
                        "command exited with status {:?}",
                        receipt.exit_code
                    ))
                },
                receipt: Some(receipt),
                send_state,
            }
        }
        TargetOutcomeKind::Failed(failure) => HostExecTargetResult {
            host,
            working_dir,
            status: HostExecTargetStatus::Failed,
            receipt: None,
            error: Some(failure.error().to_string()),
            send_state: failure.send_state(),
        },
        TargetOutcomeKind::Cancelled => HostExecTargetResult {
            host,
            working_dir,
            status: HostExecTargetStatus::Cancelled,
            receipt: None,
            error: Some("target was cancelled after fanout admission".into()),
            send_state: MutationSendState::Unknown,
        },
        TargetOutcomeKind::TimedOut => HostExecTargetResult {
            host,
            working_dir,
            status: HostExecTargetStatus::TimedOut,
            receipt: None,
            error: Some("target exceeded its bounded execution timeout".into()),
            send_state: MutationSendState::Unknown,
        },
    }
}

fn count(results: &[HostExecTargetResult], status: HostExecTargetStatus) -> usize {
    results
        .iter()
        .filter(|result| result.status == status)
        .count()
}

fn aggregate_send_state(results: &[HostExecTargetResult]) -> MutationSendState {
    if results
        .iter()
        .any(|result| result.send_state == MutationSendState::Unknown)
    {
        MutationSendState::Unknown
    } else if results
        .iter()
        .any(|result| result.send_state == MutationSendState::Sent)
    {
        MutationSendState::Sent
    } else {
        MutationSendState::NotSent
    }
}

#[cfg(test)]
#[path = "host_exec_many_tests.rs"]
mod tests;
