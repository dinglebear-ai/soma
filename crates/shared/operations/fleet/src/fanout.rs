use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures::{StreamExt, stream};
use tokio_util::sync::CancellationToken;

use crate::{HostId, HostRecord};

/// Bounds for concurrent target execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanoutPolicy {
    max_concurrency: usize,
    per_target_timeout: Duration,
}

impl FanoutPolicy {
    /// Creates a bounded fanout policy.
    pub fn new(
        max_concurrency: usize,
        per_target_timeout: Duration,
    ) -> Result<Self, FanoutPolicyError> {
        if max_concurrency == 0 {
            return Err(FanoutPolicyError::ZeroConcurrency);
        }
        if per_target_timeout.is_zero() {
            return Err(FanoutPolicyError::ZeroTimeout);
        }
        Ok(Self {
            max_concurrency,
            per_target_timeout,
        })
    }

    /// Returns maximum in-flight targets.
    #[must_use]
    pub const fn max_concurrency(self) -> usize {
        self.max_concurrency
    }

    /// Returns the timeout applied after one target begins execution.
    #[must_use]
    pub const fn per_target_timeout(self) -> Duration {
        self.per_target_timeout
    }
}

/// Invalid bounded fanout policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FanoutPolicyError {
    /// At least one target must be allowed to execute.
    #[error("fanout concurrency must be greater than zero")]
    ZeroConcurrency,
    /// Per-target timeout must be positive.
    #[error("fanout per-target timeout must be greater than zero")]
    ZeroTimeout,
}

/// Terminal classification for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetOutcomeKind<T, E> {
    /// The target completed successfully.
    Succeeded(T),
    /// The target completed with a driver or operation error.
    Failed(E),
    /// Cancellation was observed before completion.
    Cancelled,
    /// The target exceeded the configured per-target timeout.
    TimedOut,
}

/// Stable-order result for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetOutcome<T, E> {
    index: usize,
    host: HostId,
    kind: TargetOutcomeKind<T, E>,
}

impl<T, E> TargetOutcome<T, E> {
    /// Returns the original target index.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Returns the target host identity.
    #[must_use]
    pub fn host(&self) -> &HostId {
        &self.host
    }

    /// Returns the terminal classification.
    #[must_use]
    pub fn kind(&self) -> &TargetOutcomeKind<T, E> {
        &self.kind
    }
}

/// Complete stable-order fanout report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanoutReport<T, E> {
    outcomes: Vec<TargetOutcome<T, E>>,
}

impl<T, E> FanoutReport<T, E> {
    /// Returns outcomes in original target order.
    #[must_use]
    pub fn outcomes(&self) -> &[TargetOutcome<T, E>] {
        &self.outcomes
    }

    /// Returns successful target count.
    #[must_use]
    pub fn success_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome.kind, TargetOutcomeKind::Succeeded(_)))
            .count()
    }

    /// Returns failed target count.
    #[must_use]
    pub fn failure_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome.kind, TargetOutcomeKind::Failed(_)))
            .count()
    }

    /// Returns cancelled target count.
    #[must_use]
    pub fn cancelled_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome.kind, TargetOutcomeKind::Cancelled))
            .count()
    }

    /// Returns timed-out target count.
    #[must_use]
    pub fn timed_out_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome.kind, TargetOutcomeKind::TimedOut))
            .count()
    }

    /// Returns whether every target succeeded.
    #[must_use]
    pub fn all_succeeded(&self) -> bool {
        self.success_count() == self.outcomes.len()
    }
}

/// Cancellation-aware stable-order bounded fanout scheduler.
#[derive(Debug, Clone, Copy)]
pub struct FanoutScheduler {
    policy: FanoutPolicy,
}

impl FanoutScheduler {
    /// Creates a scheduler from validated bounds.
    #[must_use]
    pub const fn new(policy: FanoutPolicy) -> Self {
        Self { policy }
    }

    /// Executes one operation for every target with bounded concurrency.
    ///
    /// Returned outcomes are sorted back into the caller's target order even
    /// when faster targets complete first. Pending targets become cancelled
    /// after the shared token is cancelled instead of disappearing.
    pub async fn run<T, E, F, Fut>(
        &self,
        targets: Vec<HostRecord>,
        cancellation: CancellationToken,
        operation: F,
    ) -> FanoutReport<T, E>
    where
        T: Send,
        E: Send,
        F: Fn(HostRecord, CancellationToken) -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        let operation = Arc::new(operation);
        let timeout = self.policy.per_target_timeout;
        let mut outcomes = stream::iter(targets.into_iter().enumerate())
            .map(|(index, host)| {
                let operation = Arc::clone(&operation);
                let child = cancellation.child_token();
                async move {
                    let host_id = host.id().clone();
                    let future = operation(host, child.clone());
                    let kind = tokio::select! {
                        () = child.cancelled() => TargetOutcomeKind::Cancelled,
                        result = tokio::time::timeout(timeout, future) => match result {
                            Ok(Ok(value)) => TargetOutcomeKind::Succeeded(value),
                            Ok(Err(error)) => TargetOutcomeKind::Failed(error),
                            Err(_) => {
                                child.cancel();
                                TargetOutcomeKind::TimedOut
                            }
                        }
                    };
                    TargetOutcome {
                        index,
                        host: host_id,
                        kind,
                    }
                }
            })
            .buffer_unordered(self.policy.max_concurrency)
            .collect::<Vec<_>>()
            .await;
        outcomes.sort_by_key(TargetOutcome::index);
        FanoutReport { outcomes }
    }
}

#[cfg(test)]
#[path = "fanout_tests.rs"]
mod tests;
