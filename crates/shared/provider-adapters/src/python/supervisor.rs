//! Serial persistent Python worker supervision.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::Value;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite},
    net::TcpListener,
    process::{Child, Command},
    sync::{Mutex, Notify, OwnedSemaphorePermit},
    task::JoinHandle,
    time::timeout,
};

use crate::{
    python::{
        PythonInterpreter,
        containment::{BrokeredLaunch, CgroupGuard},
        host::{PythonExecutionProfile, PythonHostAuditEvent, PythonHostBroker},
    },
    python_protocol::{
        PythonInvocationRequest, PythonInvocationState, PythonRequestState, PythonRunnerFeature,
        PythonRunnerHostCall, PythonRunnerHostMessage, PythonRunnerHostRequest,
        PythonRunnerProtocolVersion, PythonRunnerReply, PythonRunnerWorkerMessage,
        negotiate_runner_features,
    },
    sidecar::{resolve_sidecar_command, sidecar_base_env},
};

#[cfg(test)]
#[path = "supervisor_cancel_tests.rs"]
mod cancel_tests;
#[path = "supervisor/cancellation.rs"]
mod cancellation;
#[path = "supervisor_frames.rs"]
mod frames;
#[path = "supervisor_logs.rs"]
mod logs;
#[path = "supervisor_state.rs"]
mod state;
#[path = "supervisor/status.rs"]
mod status;
#[path = "supervisor_termination.rs"]
mod termination;
use frames::{host_call_invocation_id, host_call_request_id, read_frame, write_frame};
use logs::drain_stderr;
#[cfg(test)]
use state::worker_budget_keys_are_live;
use state::{
    BusyGuard, candidate_budget, invalid_output, map_worker_error, protocol_error, start_error,
    worker_budget,
};
pub use state::{PythonInvocationOptions, PythonWorkerLogEntry};
use termination::{JobGuard, ProcessTreeStartupGuard, terminate_process_tree, terminate_worker};

/// Product-neutral limits for one persistent worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonSupervisorConfig {
    pub startup_timeout: Duration,
    pub request_timeout: Duration,
    pub shutdown_grace: Duration,
    pub max_restarts: u32,
    pub restart_window: Duration,
    pub restart_backoff: Duration,
    pub max_stderr_bytes: usize,
    pub max_pending_bytes: usize,
    pub max_workers: usize,
    pub max_candidate_starts: usize,
    pub execution_profile: PythonExecutionProfile,
}

impl Default for PythonSupervisorConfig {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(10),
            shutdown_grace: Duration::from_secs(2),
            max_restarts: 3,
            restart_window: Duration::from_secs(60),
            restart_backoff: Duration::from_millis(250),
            max_stderr_bytes: 64 * 1024,
            max_pending_bytes: 512 * 1024,
            max_workers: 32,
            max_candidate_starts: 4,
            execution_profile: PythonExecutionProfile::Trusted,
        }
    }
}

/// Immutable identity of a worker process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonWorkerIdentity {
    pub path: PathBuf,
    pub generation_id: String,
    pub worker_group: String,
    pub source_digest: String,
    pub catalog_fingerprint: String,
}

/// Operator-facing state for one persistent Python provider worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonWorkerStatus {
    pub provider_source: PathBuf,
    pub generation_id: String,
    pub running: bool,
    pub accepting: bool,
    pub busy: bool,
    pub quarantined: bool,
    pub restart_count: usize,
    pub logs: Vec<PythonWorkerLogEntry>,
    pub execution_profile: PythonExecutionProfile,
    pub host_audit: Vec<PythonHostAuditEvent>,
}

#[derive(Default)]
struct WorkerLogBuffer {
    entries: VecDeque<PythonWorkerLogEntry>,
    retained_bytes: usize,
    next_sequence: u64,
}

#[derive(Debug)]
pub struct PythonSupervisorError {
    code: &'static str,
    message: String,
}

impl PythonSupervisorError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

pub(super) struct Worker {
    pub(super) child: Child,
    pub(super) child_pid: Option<u32>,
    _job_guard: JobGuard,
    _cgroup_guard: CgroupGuard,
    _worker_permit: OwnedSemaphorePermit,
    stdin: Box<dyn AsyncWrite + Unpin + Send>,
    stdout: Box<dyn AsyncRead + Unpin + Send>,
    pub(super) stderr_task: JoinHandle<()>,
    described: bool,
    provider_path: PathBuf,
}

/// One persistent worker per Python provider. Invocations are deliberately
/// serial; callers receive a stable busy error instead of entering a queue.
pub struct PythonWorkerSupervisor {
    identity: PythonWorkerIdentity,
    interpreter: PythonInterpreter,
    config: PythonSupervisorConfig,
    worker: Mutex<Option<Worker>>,
    busy: AtomicBool,
    accepting: AtomicBool,
    dispatch_leases: AtomicUsize,
    leases_released: Notify,
    request_id: AtomicU64,
    restarts: StdMutex<VecDeque<Instant>>,
    quarantined: AtomicBool,
    started_once: AtomicBool,
    discard_worker: AtomicBool,
    cancel_epoch: AtomicU64,
    active_pid: Arc<AtomicU32>,
    logs: Arc<StdMutex<WorkerLogBuffer>>,
    host: Arc<PythonHostBroker>,
}

impl PythonWorkerSupervisor {
    #[must_use]
    pub fn new(
        identity: PythonWorkerIdentity,
        interpreter: PythonInterpreter,
        config: PythonSupervisorConfig,
    ) -> Arc<Self> {
        Self::new_with_capabilities(
            identity,
            interpreter,
            config,
            &soma_provider_core::HostCapabilities::default(),
        )
    }

    #[must_use]
    pub fn new_with_capabilities(
        identity: PythonWorkerIdentity,
        interpreter: PythonInterpreter,
        config: PythonSupervisorConfig,
        capabilities: &soma_provider_core::HostCapabilities,
    ) -> Arc<Self> {
        let host = PythonHostBroker::new(
            config.execution_profile,
            capabilities,
            Arc::new(AtomicBool::new(false)),
        );
        Arc::new(Self {
            identity,
            interpreter,
            config,
            worker: Mutex::new(None),
            busy: AtomicBool::new(false),
            accepting: AtomicBool::new(true),
            dispatch_leases: AtomicUsize::new(0),
            leases_released: Notify::new(),
            request_id: AtomicU64::new(1),
            restarts: StdMutex::new(VecDeque::new()),
            quarantined: AtomicBool::new(false),
            started_once: AtomicBool::new(false),
            discard_worker: AtomicBool::new(false),
            cancel_epoch: AtomicU64::new(0),
            active_pid: Arc::new(AtomicU32::new(0)),
            logs: Arc::new(StdMutex::new(WorkerLogBuffer::default())),
            host,
        })
    }

    /// Clears a crash-loop quarantine after an explicit operator action.
    pub async fn reset_quarantine(&self) {
        self.quarantined.store(false, Ordering::Release);
        self.started_once.store(false, Ordering::Release);
        self.restarts
            .lock()
            .expect("Python worker restart lock should not be poisoned")
            .clear();
        self.discard_worker.store(true, Ordering::Release);
    }

    /// Stops new work while allowing an invocation that already owns the
    /// worker to complete on this generation.
    pub fn deactivate(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    /// Re-enables a retained generation during atomic rollback.
    pub fn activate(&self) {
        self.accepting.store(true, Ordering::Release);
    }

    /// Reserves a call while a registry generation is still active.
    pub fn acquire_dispatch(&self) -> bool {
        if !self.accepting.load(Ordering::Acquire) {
            return false;
        }
        self.dispatch_leases.fetch_add(1, Ordering::AcqRel);
        if self.accepting.load(Ordering::Acquire) {
            return true;
        }
        self.release_dispatch();
        false
    }

    /// Releases a call reservation after the routed invocation completes.
    pub fn release_dispatch(&self) {
        let previous = self.dispatch_leases.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "dispatch lease count must not underflow");
        if previous == 1 {
            self.leases_released.notify_waiters();
        }
    }

    /// Permanently parks this generation after all already-routed calls drain.
    pub async fn suspend(&self) {
        self.deactivate();
        while self.dispatch_leases.load(Ordering::Acquire) != 0 {
            let notified = self.leases_released.notified();
            if self.dispatch_leases.load(Ordering::Acquire) == 0 {
                break;
            }
            notified.await;
        }
        self.shutdown().await;
    }

    pub async fn preflight(&self) -> Result<Value, PythonSupervisorError> {
        let _candidate_permit = candidate_budget(self.config.max_candidate_starts)
            .acquire_owned()
            .await
            .map_err(|_| start_error())?;
        let mut worker = self.worker.lock().await;
        self.ensure_worker(&mut worker).await
    }

    pub async fn invoke(
        &self,
        provider: &str,
        action: &str,
        arguments: Value,
        surface: soma_provider_core::ProviderSurface,
        snapshot_id: &str,
        timeout_override: Duration,
    ) -> Result<Value, PythonSupervisorError> {
        let context = soma_provider_core::ProviderInvocationContext::default();
        self.invoke_with_context(
            provider,
            action,
            arguments,
            PythonInvocationOptions {
                surface,
                snapshot_id,
                timeout: timeout_override,
                context: &context,
            },
        )
        .await
    }

    pub async fn invoke_with_context(
        &self,
        provider: &str,
        action: &str,
        arguments: Value,
        options: PythonInvocationOptions<'_>,
    ) -> Result<Value, PythonSupervisorError> {
        if self.config.execution_profile == PythonExecutionProfile::Disabled {
            return Err(PythonSupervisorError::new(
                "python_execution_disabled",
                "Python provider execution is disabled by policy",
            ));
        }
        if !self.accepting.load(Ordering::Acquire)
            && self.dispatch_leases.load(Ordering::Acquire) == 0
        {
            return Err(PythonSupervisorError::new(
                "python_worker_draining",
                "Python provider generation is draining",
            ));
        }
        let cancel_epoch = self.cancel_epoch.load(Ordering::Acquire);
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(PythonSupervisorError::new(
                "python_provider_busy",
                "Python provider is busy",
            ));
        }
        let mut busy = BusyGuard::new(&self.busy, &self.discard_worker);
        let result = self
            .invoke_inner((provider, action), arguments, options, cancel_epoch)
            .await;
        busy.complete();
        result
    }

    async fn invoke_inner(
        &self,
        target: (&str, &str),
        arguments: Value,
        options: PythonInvocationOptions<'_>,
        cancel_epoch: u64,
    ) -> Result<Value, PythonSupervisorError> {
        let (provider, action) = target;
        let PythonInvocationOptions {
            surface,
            snapshot_id,
            timeout: timeout_override,
            context,
        } = options;
        let encoded_len = serde_json::to_vec(&arguments)
            .map_err(|_| invalid_output())?
            .len();
        if encoded_len > self.config.max_pending_bytes {
            return Err(PythonSupervisorError::new(
                "python_input_too_large",
                "Python provider input exceeds the persistent runner limit",
            ));
        }
        let mut slot = self.worker.lock().await;
        if self.discard_worker.swap(false, Ordering::AcqRel) {
            self.active_pid.store(0, Ordering::Release);
            terminate_worker(slot.take()).await;
        }
        self.ensure_worker(&mut slot).await?;
        if self.cancel_epoch.load(Ordering::Acquire) != cancel_epoch {
            self.active_pid.store(0, Ordering::Release);
            terminate_worker(slot.take()).await;
            return Err(PythonSupervisorError::new(
                "python_provider_cancelled",
                "Python provider invocation was cancelled",
            ));
        }
        let worker = slot.as_mut().expect("worker was ensured");
        let request_id = self.next_request_id();
        let invocation_id = if context.request_id.is_empty() {
            format!("{}-{request_id}", self.identity.generation_id)
        } else {
            context.request_id.clone()
        };
        self.host.begin_invocation();
        let wait = self.config.request_timeout.min(timeout_override);
        let deadline = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_add(wait)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let actor_context =
            context
                .actor_id
                .as_ref()
                .map(|actor_id| crate::python_protocol::PythonActorContext {
                    actor_id: actor_id.clone(),
                    scopes: context.actor_scopes.clone(),
                });
        let request = PythonRunnerHostMessage::Request {
            request: PythonRunnerHostRequest::Invoke {
                request_id,
                invocation: Box::new(PythonInvocationRequest {
                    invocation_id: invocation_id.clone(),
                    request_id: invocation_id.clone(),
                    provider: provider.to_owned(),
                    action: action.to_owned(),
                    arguments,
                    surface,
                    snapshot_id: snapshot_id.to_owned(),
                    deadline_unix_ms: deadline,
                    trace: context.traceparent.as_ref().map(|traceparent| {
                        crate::python_protocol::PythonTraceContext {
                            traceparent: traceparent.clone(),
                            tracestate: context.tracestate.clone(),
                        }
                    }),
                    actor: actor_context.clone(),
                    cancellation_token_id: format!("cancel-{request_id}"),
                    generation_id: self.identity.generation_id.clone(),
                }),
            },
        };
        let exchange = async {
            write_frame(&mut worker.stdin, &request).await?;
            let mut state = PythonRequestState::Written;
            loop {
                match read_frame::<PythonRunnerWorkerMessage, _>(&mut worker.stdout).await? {
                    PythonRunnerWorkerMessage::Reply {
                        reply:
                            PythonRunnerReply::Accepted {
                                request_id: actual,
                                invocation_id: actual_invocation_id,
                                state: PythonInvocationState::Accepted,
                            },
                    } if actual == request_id
                        && actual_invocation_id == invocation_id
                        && state == PythonRequestState::Written =>
                    {
                        state = PythonRequestState::Accepted;
                        continue;
                    }
                    PythonRunnerWorkerMessage::Reply {
                        reply:
                            PythonRunnerReply::Ok {
                                request_id: actual,
                                result,
                            },
                    } if actual == request_id && state == PythonRequestState::Accepted => {
                        return Ok(result);
                    }
                    PythonRunnerWorkerMessage::Reply {
                        reply:
                            PythonRunnerReply::Error {
                                request_id: actual,
                                error,
                            },
                    } if actual == request_id && state == PythonRequestState::Accepted => {
                        return Err(map_worker_error(error.code));
                    }
                    PythonRunnerWorkerMessage::HostCall { call } => {
                        let host_request_id = host_call_request_id(&call);
                        if host_call_invocation_id(&call) != invocation_id {
                            return Err(protocol_error());
                        }
                        let (result, error) =
                            match self.host.execute(&call, actor_context.as_ref()).await {
                                Ok(result) => {
                                    if let PythonRunnerHostCall::Progress {
                                        current,
                                        total,
                                        message,
                                        ..
                                    } = &call
                                    {
                                        context.progress.report(
                                            *current,
                                            *total,
                                            message.as_deref(),
                                        );
                                    }
                                    (Some(result), None)
                                }
                                Err(error) => (None, Some(*error)),
                            };
                        write_frame(
                            &mut worker.stdin,
                            &PythonRunnerHostMessage::HostReply {
                                request_id: host_request_id,
                                result,
                                error,
                            },
                        )
                        .await?;
                    }
                    _ => return Err(protocol_error()),
                }
            }
        };
        match timeout(wait, exchange).await {
            Ok(Ok(_)) if self.cancel_epoch.load(Ordering::Acquire) != cancel_epoch => {
                self.active_pid.store(0, Ordering::Release);
                terminate_worker(slot.take()).await;
                Err(PythonSupervisorError::new(
                    "python_provider_cancelled",
                    "Python provider invocation was cancelled",
                ))
            }
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) if self.cancel_epoch.load(Ordering::Acquire) != cancel_epoch => {
                self.active_pid.store(0, Ordering::Release);
                terminate_worker(slot.take()).await;
                Err(PythonSupervisorError::new(
                    "python_provider_cancelled",
                    "Python provider invocation was cancelled",
                ))
            }
            Ok(Err(error))
                if error.code() == "python_provider_failed"
                    || error.code() == "python_provider_cancelled"
                    || error.code() == "python_output_too_large" =>
            {
                Err(error)
            }
            Ok(Err(error)) => {
                self.active_pid.store(0, Ordering::Release);
                terminate_worker(slot.take()).await;
                Err(error)
            }
            Err(_) => {
                self.active_pid.store(0, Ordering::Release);
                terminate_worker(slot.take()).await;
                Err(PythonSupervisorError::new(
                    "python_provider_timeout",
                    "Python provider exceeded its timeout",
                ))
            }
        }
    }

    async fn ensure_worker(
        &self,
        slot: &mut Option<Worker>,
    ) -> Result<Value, PythonSupervisorError> {
        if self.quarantined.load(Ordering::Acquire) {
            return Err(PythonSupervisorError::new(
                "python_provider_quarantined",
                "Python provider is quarantined after repeated worker failures",
            ));
        }
        let worker_exited = slot.as_mut().is_some_and(|worker| {
            worker
                .child
                .try_wait()
                .map_or(true, |status| status.is_some())
        });
        if worker_exited {
            self.active_pid.store(0, Ordering::Release);
            terminate_worker(slot.take()).await;
        }
        if slot.is_none() {
            self.verify_source_digest()?;
            let restarting = self.started_once.swap(true, Ordering::AcqRel);
            if restarting {
                self.record_restart()?;
            }
            if restarting && !self.config.restart_backoff.is_zero() {
                tokio::time::sleep(self.config.restart_backoff).await;
            }
            *slot = Some(self.spawn_worker().await?);
        }
        let worker = slot.as_mut().expect("worker exists");
        if worker.described {
            return Ok(Value::Null);
        }
        let request_id = self.next_request_id();
        let describe = PythonRunnerHostMessage::Request {
            request: PythonRunnerHostRequest::Describe {
                request_id,
                path: worker.provider_path.clone(),
                generation_id: self.identity.generation_id.clone(),
            },
        };
        write_frame(&mut worker.stdin, &describe).await?;
        let described = match timeout(
            self.config.startup_timeout,
            read_frame::<PythonRunnerWorkerMessage, _>(&mut worker.stdout),
        )
        .await
        {
            Ok(Ok(PythonRunnerWorkerMessage::Reply {
                reply:
                    PythonRunnerReply::Ok {
                        request_id: actual,
                        result,
                    },
            })) if actual == request_id => result,
            _ => {
                self.active_pid.store(0, Ordering::Release);
                terminate_worker(slot.take()).await;
                return Err(PythonSupervisorError::new(
                    "python_worker_start_failed",
                    "Python worker failed provider preflight",
                ));
            }
        };
        if let Err(error) = self.verify_source_digest() {
            self.active_pid.store(0, Ordering::Release);
            terminate_worker(slot.take()).await;
            return Err(error);
        }
        let manifest = match soma_provider_core::validate_provider_manifest_value(&described) {
            Ok(manifest) => manifest,
            Err(_) => {
                self.active_pid.store(0, Ordering::Release);
                terminate_worker(slot.take()).await;
                return Err(protocol_error());
            }
        };
        let actual_catalog =
            super::python_catalog_fingerprint(&manifest).map_err(|_| protocol_error())?;
        if !self.identity.catalog_fingerprint.is_empty()
            && actual_catalog != self.identity.catalog_fingerprint
        {
            self.active_pid.store(0, Ordering::Release);
            terminate_worker(slot.take()).await;
            return Err(PythonSupervisorError::new(
                "python_catalog_changed",
                "Python provider catalog changed during worker activation",
            ));
        }
        if self.health_check(worker).await.is_err() {
            self.active_pid.store(0, Ordering::Release);
            terminate_worker(slot.take()).await;
            return Err(start_error());
        }
        worker.described = true;
        Ok(described)
    }

    async fn health_check(&self, worker: &mut Worker) -> Result<(), PythonSupervisorError> {
        let request_id = self.next_request_id();
        write_frame(
            &mut worker.stdin,
            &PythonRunnerHostMessage::Request {
                request: PythonRunnerHostRequest::Health { request_id },
            },
        )
        .await?;
        match timeout(
            self.config.startup_timeout,
            read_frame::<PythonRunnerWorkerMessage, _>(&mut worker.stdout),
        )
        .await
        {
            Ok(Ok(PythonRunnerWorkerMessage::Reply {
                reply:
                    PythonRunnerReply::Health {
                        request_id: actual,
                        health: crate::python_protocol::PythonWorkerHealth::Ready,
                        generation_id,
                    },
            })) if actual == request_id && generation_id == self.identity.generation_id => Ok(()),
            _ => Err(start_error()),
        }
    }

    async fn spawn_worker(&self) -> Result<Worker, PythonSupervisorError> {
        let command = match &self.interpreter {
            PythonInterpreter::Ambient => crate::python::default_python_command().to_owned(),
            PythonInterpreter::Prepared(path) => path.to_string_lossy().into_owned(),
        };
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| start_error())?;
        let address = listener.local_addr().map_err(|_| start_error())?;
        let brokered = self.config.execution_profile == PythonExecutionProfile::Brokered;
        let (mut process, brokered_launch, worker_provider_path) = if brokered {
            let (process, launch, worker_provider_path) = BrokeredLaunch::prepare(
                resolve_sidecar_command(&command)
                    .to_str()
                    .ok_or_else(start_error)?,
                &self.identity.path,
            )?;
            (process, Some(launch), worker_provider_path)
        } else {
            (
                Command::new(resolve_sidecar_command(&command)),
                None,
                self.identity.path.clone(),
            )
        };
        let token = {
            let mut token = [0_u8; 32];
            getrandom::fill(&mut token).map_err(|_| start_error())?;
            token
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        if !brokered {
            process.args(["-I", "-m", "soma_provider.runner"]);
        }
        process
            .kill_on_drop(true)
            .env_clear()
            .env("SOMA_PYTHON_RUNNER_TOKEN", &token)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        if brokered {
            process.env("SOMA_PYTHON_RUNNER_ADDR", "unix:/run/soma/control.sock");
        } else {
            process.env("SOMA_PYTHON_RUNNER_ADDR", address.to_string());
        }
        #[cfg(unix)]
        process.process_group(0);
        for (key, value) in sidecar_base_env() {
            process.env(key, value);
        }
        // Atomic publication may briefly own both the active and replacement
        // generations. `max_workers` remains the per-generation bound.
        let worker_permit = timeout(
            self.config.startup_timeout,
            worker_budget(&self.identity.worker_group, self.config.max_workers).acquire_owned(),
        )
        .await
        .map_err(|_| start_error())?
        .map_err(|_| start_error())?;
        if let Some(launch) = &brokered_launch {
            launch.retain_until_spawn();
        }
        let mut child = crate::sidecar::spawn_retrying_busy_image(&mut process)
            .await
            .map_err(|_| {
                PythonSupervisorError::new(
                    "python_worker_start_failed",
                    "Python worker could not be started",
                )
            })?;
        let child_pid = child.id();
        let mut startup_guard = ProcessTreeStartupGuard::new(child_pid);
        let stderr = child.stderr.take().ok_or_else(protocol_error)?;
        let stderr_task = tokio::spawn(drain_stderr(
            stderr,
            self.logs.clone(),
            self.config.max_stderr_bytes,
        ));
        let cgroup_guard = match CgroupGuard::attach(child_pid, self.config.execution_profile) {
            Ok(guard) => guard,
            Err(error) => {
                terminate_process_tree(child_pid);
                let _ = child.kill().await;
                return Err(error);
            }
        };
        let job_guard = JobGuard::new(&child)?;
        let (mut stdout, stdin): (
            Box<dyn AsyncRead + Unpin + Send>,
            Box<dyn AsyncWrite + Unpin + Send>,
        ) = if let Some(launch) = &brokered_launch {
            #[cfg(target_os = "linux")]
            {
                let stream = match timeout(self.config.startup_timeout, launch.accept()).await {
                    Ok(Ok(stream)) => stream,
                    result => {
                        terminate_process_tree(child_pid);
                        let _ = child.kill().await;
                        let _ = stderr_task.await;
                        let detail = self
                            .logs
                            .lock()
                            .expect("Python worker log lock should not be poisoned")
                            .entries
                            .back()
                            .map(|entry| entry.message.clone())
                            .unwrap_or_else(|| match result {
                                Ok(Err(error)) => error.to_string(),
                                Err(_) => "control connection timed out".to_owned(),
                                Ok(Ok(_)) => unreachable!(),
                            });
                        return Err(PythonSupervisorError::new(
                            "python_worker_start_failed",
                            format!(
                                "Python worker could not connect to its control socket: {detail}"
                            ),
                        ));
                    }
                };
                let (read, write) = stream.into_split();
                (Box::new(read), Box::new(write))
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = launch;
                return Err(start_error());
            }
        } else {
            let (stream, _) = timeout(self.config.startup_timeout, listener.accept())
                .await
                .map_err(|_| start_error())?
                .map_err(|_| start_error())?;
            let (read, write) = stream.into_split();
            (Box::new(read), Box::new(write))
        };
        drop(brokered_launch);
        let mut actual_token = vec![0_u8; token.len()];
        timeout(
            self.config.startup_timeout,
            stdout.read_exact(&mut actual_token),
        )
        .await
        .map_err(|_| start_error())?
        .map_err(|_| start_error())?;
        if actual_token != token.as_bytes() {
            terminate_process_tree(child_pid);
            let _ = child.kill().await;
            return Err(protocol_error());
        }
        let mut worker = Worker {
            child,
            child_pid,
            _job_guard: job_guard,
            _cgroup_guard: cgroup_guard,
            _worker_permit: worker_permit,
            stdin,
            stdout,
            stderr_task,
            described: false,
            provider_path: worker_provider_path,
        };
        startup_guard.disarm();
        let hello = timeout(
            self.config.startup_timeout,
            read_frame::<PythonRunnerWorkerMessage, _>(&mut worker.stdout),
        )
        .await
        .map_err(|_| {
            PythonSupervisorError::new(
                "python_worker_start_failed",
                "Python worker handshake timed out",
            )
        })?
        .map_err(|_| protocol_error())?;
        let (protocol, features) = match hello {
            PythonRunnerWorkerMessage::Hello {
                protocol, features, ..
            } => (protocol, features),
            _ => return Err(protocol_error()),
        };
        let protocol = PythonRunnerProtocolVersion::current()
            .negotiate(protocol)
            .map_err(|_| protocol_error())?;
        let requested = [
            PythonRunnerFeature::Describe,
            PythonRunnerFeature::Invoke,
            PythonRunnerFeature::Health,
            PythonRunnerFeature::Drain,
            PythonRunnerFeature::Shutdown,
            PythonRunnerFeature::HostCalls,
        ];
        let features = negotiate_runner_features(&requested, &features);
        if features != requested {
            self.active_pid.store(0, Ordering::Release);
            terminate_worker(Some(worker)).await;
            return Err(protocol_error());
        }
        write_frame(
            &mut worker.stdin,
            &PythonRunnerHostMessage::Initialize {
                protocol,
                features: features.clone(),
                generation_id: self.identity.generation_id.clone(),
            },
        )
        .await?;
        match timeout(
            self.config.startup_timeout,
            read_frame::<PythonRunnerWorkerMessage, _>(&mut worker.stdout),
        )
        .await
        {
            Ok(Ok(PythonRunnerWorkerMessage::Ready {
                protocol: ready_protocol,
                features: ready_features,
                generation_id,
            })) if ready_protocol == protocol
                && ready_features == features
                && generation_id == self.identity.generation_id =>
            {
                self.active_pid
                    .store(worker.child_pid.unwrap_or_default(), Ordering::Release);
                Ok(worker)
            }
            _ => {
                self.active_pid.store(0, Ordering::Release);
                terminate_worker(Some(worker)).await;
                Err(protocol_error())
            }
        }
    }

    fn record_restart(&self) -> Result<(), PythonSupervisorError> {
        let now = Instant::now();
        let mut restarts = self
            .restarts
            .lock()
            .expect("Python worker restart lock should not be poisoned");
        while restarts
            .front()
            .is_some_and(|started| now.duration_since(*started) > self.config.restart_window)
        {
            restarts.pop_front();
        }
        if restarts.len() >= self.config.max_restarts as usize {
            self.quarantined.store(true, Ordering::Release);
            return Err(PythonSupervisorError::new(
                "python_provider_quarantined",
                "Python provider is quarantined after repeated worker failures",
            ));
        }
        restarts.push_back(now);
        Ok(())
    }

    fn current_restart_count(&self) -> usize {
        let now = Instant::now();
        let mut restarts = self
            .restarts
            .lock()
            .expect("Python worker restart lock should not be poisoned");
        while restarts
            .front()
            .is_some_and(|started| now.duration_since(*started) > self.config.restart_window)
        {
            restarts.pop_front();
        }
        restarts.len()
    }

    fn worker_running(&self) -> bool {
        let Ok(mut slot) = self.worker.try_lock() else {
            return self.active_pid.load(Ordering::Acquire) != 0;
        };
        let Some(worker) = slot.as_mut() else {
            self.active_pid.store(0, Ordering::Release);
            return false;
        };
        match worker.child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => {
                self.active_pid.store(0, Ordering::Release);
                false
            }
        }
    }

    fn next_request_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    fn verify_source_digest(&self) -> Result<(), PythonSupervisorError> {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(&self.identity.path).map_err(|_| {
            PythonSupervisorError::new(
                "python_source_changed",
                "Python provider source could not be revalidated",
            )
        })?;
        let actual = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if actual != self.identity.source_digest {
            return Err(PythonSupervisorError::new(
                "python_source_changed",
                "Python provider source changed during worker activation",
            ));
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        let mut slot = self.worker.lock().await;
        let Some(mut worker) = slot.take() else {
            self.started_once.store(false, Ordering::Release);
            return;
        };
        let request_id = self.next_request_id();
        let request = PythonRunnerHostMessage::Request {
            request: PythonRunnerHostRequest::Shutdown { request_id },
        };
        let _ = write_frame(&mut worker.stdin, &request).await;
        let _ = timeout(self.config.shutdown_grace, worker.child.wait()).await;
        terminate_worker(Some(worker)).await;
        self.active_pid.store(0, Ordering::Release);
        self.started_once.store(false, Ordering::Release);
    }

    /// Stop accepting work, then shut the worker down within the configured
    /// grace period. Used when a registry generation is retired.
    pub async fn drain_and_shutdown(&self) {
        let mut slot = self.worker.lock().await;
        let Some(worker) = slot.as_mut() else {
            return;
        };
        let request_id = self.next_request_id();
        let _ = write_frame(
            &mut worker.stdin,
            &PythonRunnerHostMessage::Request {
                request: PythonRunnerHostRequest::Drain { request_id },
            },
        )
        .await;
        drop(slot);
        self.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path};
    use tokio::io::AsyncWriteExt;

    /// Deadline for anything these tests expect to *succeed*. Booting a Python
    /// worker on a machine running the rest of the suite in parallel is not
    /// bounded by any interesting constant, and a tight deadline here only
    /// converts scheduler pressure into a false failure. Deliberately short
    /// deadlines belong on the calls that are asserted to time out.
    const GENEROUS: Duration = Duration::from_secs(60);

    /// Polls `condition` until it reports true or `deadline` elapses, yielding
    /// between attempts. Returns whether the condition was observed.
    ///
    /// Prefer this over sleeping a fixed interval and hoping the state has
    /// settled: the sleep encodes a guess about machine speed, this encodes the
    /// actual precondition.
    async fn wait_for(deadline: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let expiry = tokio::time::Instant::now() + deadline;
        while tokio::time::Instant::now() < expiry {
            if condition() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        condition()
    }

    fn installed_test_python() -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packages/python/.venv");
        let path = if cfg!(windows) {
            root.join("Scripts/python.exe")
        } else {
            root.join("bin/python")
        };
        assert!(
            path.is_file(),
            "persistent-runner tests require `uv sync --project packages/python --frozen`"
        );
        path
    }

    fn identity(path: &Path) -> PythonWorkerIdentity {
        let source_digest = Sha256::digest(fs::read(path).expect("provider source"))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        PythonWorkerIdentity {
            path: path.to_owned(),
            generation_id: "supervisor-test-generation".to_owned(),
            worker_group: "supervisor-test-generation".to_owned(),
            source_digest,
            catalog_fingerprint: String::new(),
        }
    }

    #[tokio::test]
    async fn installed_runner_preflights_invokes_times_out_and_restarts() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("persistent.py");
        fs::write(
            &provider,
            r#"
import time
PROVIDER = {"name": "persistent-test", "kind": "python"}

def execute(value: str, delay_ms: int = 0) -> dict:
    if delay_ms:
        time.sleep(delay_ms / 1000)
    return {"value": value}
"#,
        )
        .expect("write provider");
        // `invoke` waits `config.request_timeout.min(per_call_timeout)`, so the
        // config value caps every call including the two that must succeed.
        // Keep it generous and let each call's own override decide: the slow
        // call below passes a deliberately short one to force the timeout.
        let config = PythonSupervisorConfig {
            request_timeout: GENEROUS,
            startup_timeout: GENEROUS,
            restart_backoff: Duration::ZERO,
            ..PythonSupervisorConfig::default()
        };
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            config,
        );

        let catalog = supervisor.preflight().await.expect("preflight");
        assert_eq!(catalog["provider"]["name"], "persistent-test");
        let output = supervisor
            .invoke(
                "persistent-test",
                "execute",
                json!({"value": "first"}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("first invocation");
        assert_eq!(output, json!({"value": "first"}));

        let timeout = supervisor
            .invoke(
                "persistent-test",
                "execute",
                json!({"value": "slow", "delay_ms": 300}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                Duration::from_millis(100),
            )
            .await
            .expect_err("slow invocation must time out");
        assert_eq!(timeout.code(), "python_provider_timeout");

        let restarted = supervisor
            .invoke(
                "persistent-test",
                "execute",
                json!({"value": "restarted"}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("later invocation restarts without replay");
        assert_eq!(restarted, json!({"value": "restarted"}));
        supervisor.drain_and_shutdown().await;
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires a delegated cgroup-v2 root in SOMA_PYTHON_BROKER_CGROUP_ROOT"]
    async fn brokered_worker_launches_inside_the_enforced_boundary() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("brokered.py");
        fs::write(temp.path().join("host-sentinel.txt"), "must remain hidden")
            .expect("write sentinel");
        fs::write(
            &provider,
            r#"
from pathlib import Path

try:
    Path(__file__).with_name("host-sentinel.txt").read_text()
    SENTINEL_VISIBLE = True
except OSError:
    SENTINEL_VISIBLE = False

PROVIDER = {"name": "brokered-test", "kind": "python"}
def execute(value: str) -> dict:
    return {"value": value, "sentinel_visible": SENTINEL_VISIBLE}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new_with_capabilities(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            PythonSupervisorConfig {
                execution_profile: PythonExecutionProfile::Brokered,
                ..PythonSupervisorConfig::default()
            },
            &soma_provider_core::HostCapabilities::default(),
        );
        supervisor.preflight().await.expect("brokered preflight");
        let output = supervisor
            .invoke(
                "brokered-test",
                "execute",
                json!({"value": "contained"}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("brokered invocation");
        assert_eq!(
            output,
            json!({"value": "contained", "sentinel_visible": false})
        );
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn concurrent_invocation_is_rejected_before_queueing() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("busy.py");
        fs::write(
            &provider,
            r#"
import time
PROVIDER = {"name": "busy-test", "kind": "python"}
def wait(delay_ms: int) -> dict:
    time.sleep(delay_ms / 1000)
    return {"ok": True}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            PythonSupervisorConfig::default(),
        );
        supervisor.preflight().await.expect("preflight");
        let first = {
            let supervisor = supervisor.clone();
            tokio::spawn(async move {
                supervisor
                    .invoke(
                        "busy-test",
                        "wait",
                        // Wide enough that the busy window below cannot close
                        // before the contending call is issued.
                        json!({"delay_ms": 3_000}),
                        soma_provider_core::ProviderSurface::Mcp,
                        "snapshot-a",
                        GENEROUS,
                    )
                    .await
            })
        };
        // Wait for the first call to actually occupy the worker instead of
        // assuming a fixed interval was long enough to get there.
        assert!(
            wait_for(GENEROUS, || supervisor.status().busy).await,
            "first invocation never occupied the worker"
        );
        let busy = supervisor
            .invoke(
                "busy-test",
                "wait",
                json!({"delay_ms": 0}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect_err("second invocation must not queue");
        assert_eq!(busy.code(), "python_provider_busy");
        first.await.expect("join").expect("first call");
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn active_invocation_cancels_process_tree_and_later_work_restarts() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("cancel.py");
        fs::write(
            &provider,
            r#"
import time
PROVIDER = {"name": "cancel-test", "kind": "python"}
def wait(delay_ms: int) -> dict:
    time.sleep(delay_ms / 1000)
    return {"ok": True}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            PythonSupervisorConfig {
                restart_backoff: Duration::ZERO,
                ..PythonSupervisorConfig::default()
            },
        );
        supervisor.preflight().await.expect("preflight");
        let active = {
            let supervisor = supervisor.clone();
            tokio::spawn(async move {
                supervisor
                    .invoke(
                        "cancel-test",
                        "wait",
                        json!({"delay_ms": 30_000}),
                        soma_provider_core::ProviderSurface::Mcp,
                        "snapshot-a",
                        GENEROUS,
                    )
                    .await
            })
        };
        // Same precondition race as
        // `closing_stderr_does_not_revoke_active_cancellation`: poll until the
        // invocation is genuinely cancellable rather than sleeping a guess.
        assert!(
            wait_for(GENEROUS, || supervisor.cancel_active()).await,
            "active invocation never became cancellable"
        );
        let error = active
            .await
            .expect("join")
            .expect_err("active call must cancel");
        assert_eq!(error.code(), "python_provider_cancelled");

        let restarted = supervisor
            .invoke(
                "cancel-test",
                "wait",
                json!({"delay_ms": 0}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("later work starts a clean worker");
        assert_eq!(restarted, json!({"ok": true}));
        supervisor.deactivate();
        let draining = supervisor
            .invoke(
                "cancel-test",
                "wait",
                json!({"delay_ms": 0}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect_err("retained generation must reject new work");
        assert_eq!(draining.code(), "python_worker_draining");
        supervisor.activate();
        supervisor
            .invoke(
                "cancel-test",
                "wait",
                json!({"delay_ms": 0}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("rollback activation permits new work");
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn closing_stderr_does_not_revoke_active_cancellation() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("close_stderr.py");
        fs::write(
            &provider,
            r#"
import os
import time
PROVIDER = {"name": "close-stderr-test", "kind": "python"}
def close_and_wait(delay_ms: int) -> dict:
    os.close(2)
    time.sleep(delay_ms / 1000)
    return {"ok": True}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            PythonSupervisorConfig::default(),
        );
        supervisor.preflight().await.expect("preflight");
        let active = {
            let supervisor = supervisor.clone();
            tokio::spawn(async move {
                supervisor
                    .invoke(
                        "close-stderr-test",
                        "close_and_wait",
                        // Long enough that returning within CANCEL_BOUND below
                        // can only mean cancellation cut the call short.
                        json!({"delay_ms": 30_000}),
                        soma_provider_core::ProviderSurface::Mcp,
                        "snapshot-a",
                        GENEROUS,
                    )
                    .await
            })
        };
        // `cancel_active` reports false until the invocation has both marked
        // the supervisor busy and recorded the worker pid, and both of those
        // happen after the spawn above returns. Sleeping a fixed interval
        // races that setup under load; poll the real precondition instead.
        // A false return is side-effect free (it bails before touching any
        // state), so retrying it is safe.
        assert!(
            wait_for(GENEROUS, || supervisor.status().busy).await,
            "invocation never reached the worker"
        );
        assert!(supervisor.status().running);
        assert!(
            wait_for(GENEROUS, || supervisor.cancel_active()).await,
            "active invocation never became cancellable"
        );

        // Well under the 30s the provider would otherwise sleep, so this still
        // proves cancellation short-circuited the call rather than waiting it
        // out, but with enough headroom to survive a loaded machine.
        const CANCEL_BOUND: Duration = Duration::from_secs(10);
        let error = tokio::time::timeout(CANCEL_BOUND, active)
            .await
            .expect("cancellation must not wait for the invocation to finish")
            .expect("join")
            .expect_err("active invocation is cancelled");
        assert_eq!(error.code(), "python_provider_cancelled");
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn dead_idle_worker_is_restarted_before_next_dispatch() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("idle_crash.py");
        fs::write(
            &provider,
            r#"
PROVIDER = {"name": "idle-crash-test", "kind": "python"}
def value() -> dict:
    return {"ok": True}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            PythonSupervisorConfig {
                restart_backoff: Duration::ZERO,
                ..PythonSupervisorConfig::default()
            },
        );
        supervisor.preflight().await.expect("preflight");
        let original_pid = supervisor.active_pid.load(Ordering::Acquire);
        terminate_process_tree(Some(original_pid));
        for _ in 0..100 {
            if !supervisor.status().running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            !supervisor.status().running,
            "status observes idle child death before another dispatch"
        );

        let output = supervisor
            .invoke(
                "idle-crash-test",
                "value",
                json!({}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("first post-crash invocation starts a replacement");
        assert_eq!(output, json!({"ok": true}));
        assert_ne!(supervisor.active_pid.load(Ordering::Acquire), original_pid);
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn suspension_drains_routed_calls_and_does_not_consume_restart_budget() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("generation.py");
        fs::write(
            &provider,
            r#"
PROVIDER = {"name": "generation-test", "kind": "python"}
def value() -> dict:
    return {"value": "ok"}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            PythonSupervisorConfig {
                restart_backoff: Duration::ZERO,
                ..PythonSupervisorConfig::default()
            },
        );
        supervisor.preflight().await.expect("preflight");
        assert!(supervisor.acquire_dispatch());
        let suspending = {
            let supervisor = supervisor.clone();
            tokio::spawn(async move { supervisor.suspend().await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!supervisor.acquire_dispatch());
        let routed = supervisor
            .invoke(
                "generation-test",
                "value",
                json!({}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("already-routed call drains");
        assert_eq!(routed, json!({"value": "ok"}));
        supervisor.release_dispatch();
        suspending.await.expect("suspension task");

        supervisor.activate();
        assert!(supervisor.acquire_dispatch());
        supervisor
            .invoke(
                "generation-test",
                "value",
                json!({}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-b",
                GENEROUS,
            )
            .await
            .expect("rollback starts a planned fresh worker");
        supervisor.release_dispatch();
        assert_eq!(supervisor.status().restart_count, 0);
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn worker_logs_are_bounded_structured_and_redacted() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("logs.py");
        fs::write(
            &provider,
            r#"
PROVIDER = {"name": "logs-test", "kind": "python"}
def emit() -> dict:
    print("token=super-secret")
    print("credential: unmarked-private-data")
    print("safe diagnostic")
    return {"ok": True}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            PythonSupervisorConfig {
                max_stderr_bytes: 128,
                ..PythonSupervisorConfig::default()
            },
        );
        supervisor.preflight().await.expect("preflight");
        supervisor
            .invoke(
                "logs-test",
                "emit",
                json!({}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                Duration::from_secs(1),
            )
            .await
            .expect("invoke");
        tokio::time::sleep(Duration::from_millis(20)).await;
        let status = supervisor.status();
        assert!(
            status
                .logs
                .iter()
                .all(|entry| entry.message == "[redacted provider diagnostic]")
        );
        let encoded = serde_json::to_string(&status).unwrap();
        assert!(!encoded.contains("super-secret"));
        assert!(!encoded.contains("unmarked-private-data"));
        assert!(!encoded.contains("safe diagnostic"));
        assert!(
            status
                .logs
                .iter()
                .map(|entry| entry.message.len())
                .sum::<usize>()
                <= 128
        );
        supervisor.shutdown().await;
    }

    #[test]
    fn worker_budget_is_bounded_per_immutable_generation() {
        let first = worker_budget("generation-a", 1);
        let _active = first.clone().try_acquire_owned().expect("first permit");
        assert!(first.try_acquire_owned().is_err());

        let replacement = worker_budget("generation-b", 1);
        assert!(
            replacement.try_acquire_owned().is_ok(),
            "a replacement generation has bounded overlap capacity"
        );
    }

    #[test]
    fn worker_budget_prunes_retired_generation_keys() {
        for generation in 0..128 {
            drop(worker_budget(&format!("retired-{generation}"), 1));
        }
        let _live = worker_budget("live-generation", 1);
        assert!(
            worker_budget_keys_are_live(),
            "retired generation keys must not accumulate"
        );
    }

    #[tokio::test]
    async fn restart_count_expires_without_another_restart() {
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("provider.py");
        fs::write(
            &provider,
            "PROVIDER = {\"name\": \"restart-window\", \"kind\": \"python\"}\n",
        )
        .expect("provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(PathBuf::from("/unused")),
            PythonSupervisorConfig {
                restart_window: Duration::from_millis(10),
                ..PythonSupervisorConfig::default()
            },
        );
        supervisor.record_restart().expect("record restart");
        assert_eq!(supervisor.status().restart_count, 1);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(supervisor.status().restart_count, 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_startup_terminates_the_entire_spawned_process_group() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("provider.py");
        fs::write(
            &provider,
            "PROVIDER = {\"name\": \"startup-tree\", \"kind\": \"python\"}\n",
        )
        .expect("provider");
        let descendant_pid = temp.path().join("descendant.pid");
        let fake_python = temp.path().join("fake-python");
        fs::write(
            &fake_python,
            format!(
                "#!/bin/sh\nsleep 30 &\necho $! > '{}'\nsleep 30\n",
                descendant_pid.display()
            ),
        )
        .expect("fake python");
        fs::set_permissions(&fake_python, fs::Permissions::from_mode(0o700))
            .expect("executable fake python");

        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(fake_python),
            // The fake worker never connects, so startup fails at *any*
            // timeout — this value is not the property under test. It does
            // gate how long the fake shell has to fork its descendant and
            // record the pid the assertions below read, and a 150ms budget
            // lost that race whenever the machine was busy, leaving an empty
            // pid file.
            PythonSupervisorConfig {
                startup_timeout: Duration::from_secs(5),
                ..PythonSupervisorConfig::default()
            },
        );
        let error = supervisor
            .preflight()
            .await
            .expect_err("fake worker never connects");
        assert_eq!(error.code(), "python_worker_start_failed");
        let pid: u32 = fs::read_to_string(&descendant_pid)
            .expect("descendant pid")
            .trim()
            .parse()
            .expect("numeric pid");
        for _ in 0..100 {
            if !Path::new("/proc").join(pid.to_string()).exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("descendant process {pid} survived failed worker startup");
    }

    #[tokio::test]
    async fn quarantine_exhaustion_is_visible_and_operator_reset_recovers() {
        let python = installed_test_python();
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("quarantine.py");
        fs::write(
            &provider,
            r#"
import time
PROVIDER = {"name": "quarantine-test", "kind": "python"}
def wait(delay_ms: int) -> dict:
    time.sleep(delay_ms / 1000)
    return {"ok": True}
"#,
        )
        .expect("write provider");
        let supervisor = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(python),
            // As in `installed_runner_preflights_invokes_times_out_and_restarts`,
            // `invoke` waits `request_timeout.min(per_call)`. A short config
            // value here also capped the post-reset call, which has to boot a
            // brand-new interpreter — the timeout below supplies its own short
            // per-call deadline instead.
            PythonSupervisorConfig {
                request_timeout: GENEROUS,
                max_restarts: 0,
                restart_backoff: Duration::ZERO,
                ..PythonSupervisorConfig::default()
            },
        );
        supervisor.preflight().await.expect("preflight");
        let timeout = supervisor
            .invoke(
                "quarantine-test",
                "wait",
                json!({"delay_ms": 200}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                Duration::from_millis(50),
            )
            .await
            .expect_err("slow call times out");
        assert_eq!(timeout.code(), "python_provider_timeout");
        let quarantined = supervisor
            .invoke(
                "quarantine-test",
                "wait",
                json!({"delay_ms": 0}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect_err("restart budget is exhausted");
        assert_eq!(quarantined.code(), "python_provider_quarantined");
        assert!(supervisor.status().quarantined);

        supervisor.reset_quarantine().await;
        assert!(!supervisor.status().quarantined);
        let output = supervisor
            .invoke(
                "quarantine-test",
                "wait",
                json!({"delay_ms": 0}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                GENEROUS,
            )
            .await
            .expect("operator reset permits a fresh worker");
        assert_eq!(output, json!({"ok": true}));
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn source_substitution_and_missing_runner_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("source.py");
        fs::write(
            &provider,
            "PROVIDER = {\"name\": \"source-test\", \"kind\": \"python\"}\n",
        )
        .expect("write provider");
        let source_identity = identity(&provider);
        fs::write(
            &provider,
            "PROVIDER = {\"name\": \"substituted\", \"kind\": \"python\"}\n",
        )
        .expect("substitute provider");
        let substituted = PythonWorkerSupervisor::new(
            source_identity,
            PythonInterpreter::Prepared(PathBuf::from("/missing/soma-python")),
            PythonSupervisorConfig::default(),
        );
        assert_eq!(
            substituted.preflight().await.unwrap_err().code(),
            "python_source_changed"
        );

        let missing = PythonWorkerSupervisor::new(
            identity(&provider),
            PythonInterpreter::Prepared(PathBuf::from("/missing/soma-python")),
            PythonSupervisorConfig::default(),
        );
        assert_eq!(
            missing.preflight().await.unwrap_err().code(),
            "python_worker_start_failed"
        );
    }

    #[tokio::test]
    async fn production_control_reader_rejects_malformed_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let client = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(address)
                .await
                .expect("connect");
            stream
                .write_all(&5_u32.to_be_bytes())
                .await
                .expect("header");
            stream.write_all(b"{nope").await.expect("payload");
        });
        let (stream, _) = listener.accept().await.expect("accept");
        let (mut reader, _) = stream.into_split();
        let error = read_frame::<PythonRunnerWorkerMessage, _>(&mut reader)
            .await
            .expect_err("malformed production frame must fail closed");
        assert_eq!(error.code(), "python_protocol_mismatch");
        client.await.expect("client task");
    }
}
