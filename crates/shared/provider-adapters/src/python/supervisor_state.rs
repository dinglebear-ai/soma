use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use tokio::sync::Semaphore;

use crate::python_protocol::{PythonProtocolError, PythonRunnerErrorCode};

use super::PythonSupervisorError;

/// One redacted, structured line emitted by a persistent Python worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonWorkerLogEntry {
    pub sequence: u64,
    pub stream: &'static str,
    pub message: String,
}

/// Per-call identity, policy context, and deadline for a Python invocation.
#[derive(Debug, Clone, Copy)]
pub struct PythonInvocationOptions<'a> {
    pub surface: soma_provider_core::ProviderSurface,
    pub snapshot_id: &'a str,
    pub timeout: Duration,
    pub context: &'a soma_provider_core::ProviderInvocationContext,
}

impl std::fmt::Display for PythonSupervisorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PythonSupervisorError {}

pub(super) fn start_error() -> PythonSupervisorError {
    PythonSupervisorError::new(
        "python_worker_start_failed",
        "Python worker could not be started",
    )
}

pub(super) fn worker_budget(group: &str, limit: usize) -> Arc<Semaphore> {
    let limit = limit.max(1);
    let key = (group.to_owned(), limit);
    let mut budgets = WORKER_BUDGETS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("Python worker budget lock should not be poisoned");
    budgets.retain(|_, budget| budget.strong_count() != 0);
    if let Some(budget) = budgets.get(&key).and_then(Weak::upgrade) {
        return budget;
    }
    let budget = Arc::new(Semaphore::new(limit));
    budgets.insert(key, Arc::downgrade(&budget));
    budget
}

pub(super) fn candidate_budget(limit: usize) -> Arc<Semaphore> {
    shared_budget(limit, &CANDIDATE_BUDGETS)
}

type BudgetMap = Mutex<BTreeMap<usize, Weak<Semaphore>>>;
type WorkerBudgetMap = Mutex<BTreeMap<(String, usize), Weak<Semaphore>>>;
static WORKER_BUDGETS: OnceLock<WorkerBudgetMap> = OnceLock::new();
static CANDIDATE_BUDGETS: OnceLock<BudgetMap> = OnceLock::new();

#[cfg(test)]
pub(super) fn worker_budget_keys_are_live() -> bool {
    WORKER_BUDGETS
        .get()
        .expect("worker budgets")
        .lock()
        .expect("worker budget lock")
        .values()
        .all(|budget| budget.strong_count() != 0)
}

fn shared_budget(limit: usize, budgets: &'static OnceLock<BudgetMap>) -> Arc<Semaphore> {
    let limit = limit.max(1);
    let mut budgets = budgets
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("Python worker budget lock should not be poisoned");
    budgets.retain(|_, budget| budget.strong_count() != 0);
    if let Some(budget) = budgets.get(&limit).and_then(Weak::upgrade) {
        return budget;
    }
    let budget = Arc::new(Semaphore::new(limit));
    budgets.insert(limit, Arc::downgrade(&budget));
    budget
}

pub(super) struct BusyGuard<'a> {
    busy: &'a AtomicBool,
    discard_worker: &'a AtomicBool,
    completed: bool,
}

impl<'a> BusyGuard<'a> {
    pub(super) fn new(busy: &'a AtomicBool, discard_worker: &'a AtomicBool) -> Self {
        Self {
            busy,
            discard_worker,
            completed: false,
        }
    }

    pub(super) fn complete(&mut self) {
        self.completed = true;
        self.busy.store(false, Ordering::Release);
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.discard_worker.store(true, Ordering::Release);
            self.busy.store(false, Ordering::Release);
        }
    }
}

pub(super) fn map_worker_error(code: PythonRunnerErrorCode) -> PythonSupervisorError {
    match code {
        PythonRunnerErrorCode::PythonCallTimeout => PythonSupervisorError::new(
            "python_provider_timeout",
            "Python provider exceeded its timeout",
        ),
        PythonRunnerErrorCode::PythonCallCancelled => PythonSupervisorError::new(
            "python_provider_cancelled",
            "Python provider invocation was cancelled",
        ),
        PythonRunnerErrorCode::PythonOutputTooLarge => PythonSupervisorError::new(
            "python_output_too_large",
            "Python provider output exceeded its limit",
        ),
        PythonRunnerErrorCode::PythonPolicyDenied => PythonSupervisorError::new(
            "python_policy_denied",
            "Python provider host capability was denied",
        ),
        _ => PythonSupervisorError::new(
            "python_provider_failed",
            "Python provider invocation failed",
        ),
    }
}

pub(super) fn invalid_output() -> PythonSupervisorError {
    PythonSupervisorError::new(
        "python_invalid_output",
        "Python provider produced invalid output",
    )
}

pub(super) fn protocol_error() -> PythonSupervisorError {
    PythonSupervisorError::new(
        "python_protocol_mismatch",
        "Python worker violated the runner protocol",
    )
}

impl From<PythonProtocolError> for PythonSupervisorError {
    fn from(_: PythonProtocolError) -> Self {
        protocol_error()
    }
}

impl From<std::io::Error> for PythonSupervisorError {
    fn from(_: std::io::Error) -> Self {
        PythonSupervisorError::new("python_worker_crashed", "Python worker exited unexpectedly")
    }
}
