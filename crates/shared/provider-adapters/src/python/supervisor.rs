//! Serial persistent Python worker supervision.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::Value;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    process::{Child, Command},
    sync::{Mutex, OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
    time::timeout,
};

use crate::{
    python::PythonInterpreter,
    python_protocol::{
        PYTHON_RUNNER_MAX_FRAME_BYTES, PythonInvocationRequest, PythonInvocationState,
        PythonProtocolError, PythonRequestState, PythonRunnerErrorCode, PythonRunnerFeature,
        PythonRunnerHostMessage, PythonRunnerHostRequest, PythonRunnerProtocolVersion,
        PythonRunnerReply, PythonRunnerWorkerMessage, negotiate_runner_features,
    },
    sidecar::{resolve_sidecar_command, sidecar_base_env},
};

const FRAME_HEADER_BYTES: usize = 4;

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
        }
    }
}

/// Immutable identity of a worker process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonWorkerIdentity {
    pub path: PathBuf,
    pub generation_id: String,
    pub source_digest: String,
    pub catalog_fingerprint: String,
}

/// One redacted, structured line emitted by a persistent Python worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonWorkerLogEntry {
    pub sequence: u64,
    pub stream: &'static str,
    pub message: String,
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
    message: &'static str,
}

impl PythonSupervisorError {
    pub const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl std::fmt::Display for PythonSupervisorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for PythonSupervisorError {}

struct Worker {
    child: Child,
    child_pid: Option<u32>,
    _job_guard: JobGuard,
    _worker_permit: OwnedSemaphorePermit,
    stdin: OwnedWriteHalf,
    stdout: OwnedReadHalf,
    stderr_task: JoinHandle<()>,
    described: bool,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // `Child::kill_on_drop` only covers the direct child. The worker owns
        // the whole process tree, including descendants created by provider
        // code, so rollback and failed unpublished candidates must signal it.
        terminate_process_tree(self.child_pid);
        self.stderr_task.abort();
    }
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
    request_id: AtomicU64,
    restarts: Mutex<VecDeque<Instant>>,
    quarantined: AtomicBool,
    started_once: AtomicBool,
    discard_worker: AtomicBool,
    cancel_epoch: AtomicU64,
    active_pid: AtomicU32,
    logs: Arc<StdMutex<WorkerLogBuffer>>,
}

impl PythonWorkerSupervisor {
    #[must_use]
    pub fn new(
        identity: PythonWorkerIdentity,
        interpreter: PythonInterpreter,
        config: PythonSupervisorConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            identity,
            interpreter,
            config,
            worker: Mutex::new(None),
            busy: AtomicBool::new(false),
            accepting: AtomicBool::new(true),
            request_id: AtomicU64::new(1),
            restarts: Mutex::new(VecDeque::new()),
            quarantined: AtomicBool::new(false),
            started_once: AtomicBool::new(false),
            discard_worker: AtomicBool::new(false),
            cancel_epoch: AtomicU64::new(0),
            active_pid: AtomicU32::new(0),
            logs: Arc::new(StdMutex::new(WorkerLogBuffer::default())),
        })
    }

    /// Returns a bounded, redacted snapshot without executing provider code.
    #[must_use]
    pub fn status(&self) -> PythonWorkerStatus {
        let logs = self
            .logs
            .lock()
            .expect("Python worker log lock should not be poisoned");
        let restart_count = self
            .restarts
            .try_lock()
            .map_or(0, |restarts| restarts.len());
        PythonWorkerStatus {
            provider_source: self.identity.path.clone(),
            generation_id: self.identity.generation_id.clone(),
            running: self.active_pid.load(Ordering::Acquire) != 0,
            accepting: self.accepting.load(Ordering::Acquire),
            busy: self.busy.load(Ordering::Acquire),
            quarantined: self.quarantined.load(Ordering::Acquire),
            restart_count,
            logs: logs.entries.iter().cloned().collect(),
        }
    }

    /// Deterministically cancels the active invocation by terminating its
    /// complete process tree. The next invocation starts a fresh worker.
    pub fn cancel_active(&self) -> bool {
        if !self.busy.load(Ordering::Acquire) {
            return false;
        }
        self.cancel_epoch.fetch_add(1, Ordering::AcqRel);
        let pid = self.active_pid.swap(0, Ordering::AcqRel);
        self.discard_worker.store(true, Ordering::Release);
        if pid != 0 {
            terminate_process_tree(Some(pid));
        }
        true
    }

    /// Clears a crash-loop quarantine after an explicit operator action.
    pub async fn reset_quarantine(&self) {
        self.quarantined.store(false, Ordering::Release);
        self.started_once.store(false, Ordering::Release);
        self.restarts.lock().await.clear();
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
        if !self.accepting.load(Ordering::Acquire) {
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
            .invoke_inner(
                (provider, action),
                arguments,
                surface,
                snapshot_id,
                timeout_override,
                cancel_epoch,
            )
            .await;
        busy.complete();
        result
    }

    async fn invoke_inner(
        &self,
        target: (&str, &str),
        arguments: Value,
        surface: soma_provider_core::ProviderSurface,
        snapshot_id: &str,
        timeout_override: Duration,
        cancel_epoch: u64,
    ) -> Result<Value, PythonSupervisorError> {
        let (provider, action) = target;
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
        let invocation_id = format!("{}-{request_id}", self.identity.generation_id);
        let deadline = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_add(timeout_override)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let request = PythonRunnerHostMessage::Request {
            request: PythonRunnerHostRequest::Invoke {
                request_id,
                invocation: Box::new(PythonInvocationRequest {
                    invocation_id: invocation_id.clone(),
                    provider: provider.to_owned(),
                    action: action.to_owned(),
                    arguments,
                    surface,
                    snapshot_id: snapshot_id.to_owned(),
                    deadline_unix_ms: deadline,
                    trace: None,
                    actor: None,
                    cancellation_token_id: format!("cancel-{request_id}"),
                    generation_id: self.identity.generation_id.clone(),
                }),
            },
        };
        let exchange = async {
            write_frame(&mut worker.stdin, &request).await?;
            let mut state = PythonRequestState::Written;
            loop {
                match read_frame::<PythonRunnerWorkerMessage>(&mut worker.stdout).await? {
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
                        write_frame(
                            &mut worker.stdin,
                            &PythonRunnerHostMessage::HostReply {
                                request_id: host_request_id,
                                result: None,
                                error: Some(crate::python_protocol::PythonRunnerError {
                                    code: PythonRunnerErrorCode::PythonPolicyDenied,
                                    phase: crate::python_protocol::PythonRunnerErrorPhase::Policy,
                                    provider: None,
                                    source: None,
                                    generation_id: Some(self.identity.generation_id.clone()),
                                    action: None,
                                    retryable: false,
                                    public_message:
                                        "Python host capabilities are unavailable in this phase"
                                            .to_owned(),
                                }),
                            },
                        )
                        .await?;
                    }
                    _ => return Err(protocol_error()),
                }
            }
        };
        let wait = self.config.request_timeout.min(timeout_override);
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
        if slot.is_none() {
            self.verify_source_digest()?;
            let restarting = self.started_once.swap(true, Ordering::AcqRel);
            if restarting {
                self.record_restart().await?;
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
                path: self.identity.path.clone(),
                generation_id: self.identity.generation_id.clone(),
            },
        };
        write_frame(&mut worker.stdin, &describe).await?;
        let described = match timeout(
            self.config.startup_timeout,
            read_frame::<PythonRunnerWorkerMessage>(&mut worker.stdout),
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
        let actual_catalog = {
            use sha2::{Digest, Sha256};
            Sha256::digest(serde_json::to_vec(&manifest).map_err(|_| protocol_error())?)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
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
            read_frame::<PythonRunnerWorkerMessage>(&mut worker.stdout),
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
        let mut process = Command::new(resolve_sidecar_command(&command));
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| start_error())?;
        let address = listener.local_addr().map_err(|_| start_error())?;
        let token = {
            use sha2::{Digest, Sha256};
            Sha256::digest(format!(
                "{}-{}-{:?}",
                self.identity.generation_id,
                std::process::id(),
                SystemTime::now()
            ))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
        };
        process
            .args(["-I", "-m", "soma_provider.runner"])
            .kill_on_drop(true)
            .env_clear()
            .env("SOMA_PYTHON_RUNNER_ADDR", address.to_string())
            .env("SOMA_PYTHON_RUNNER_TOKEN", &token)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        process.process_group(0);
        for (key, value) in sidecar_base_env() {
            process.env(key, value);
        }
        // Atomic publication may briefly own both the active and replacement
        // generations. `max_workers` remains the per-generation bound.
        let worker_permit = timeout(
            self.config.startup_timeout,
            worker_budget(self.config.max_workers.saturating_mul(2)).acquire_owned(),
        )
        .await
        .map_err(|_| start_error())?
        .map_err(|_| start_error())?;
        let mut child = process.spawn().map_err(|_| {
            PythonSupervisorError::new(
                "python_worker_start_failed",
                "Python worker could not be started",
            )
        })?;
        let child_pid = child.id();
        let job_guard = JobGuard::new(child_pid)?;
        let (stream, _) = timeout(self.config.startup_timeout, listener.accept())
            .await
            .map_err(|_| start_error())?
            .map_err(|_| start_error())?;
        let (mut stdout, stdin) = stream.into_split();
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
        let stderr = child.stderr.take().ok_or_else(protocol_error)?;
        let stderr_task = tokio::spawn(drain_stderr(
            stderr,
            self.logs.clone(),
            self.config.max_stderr_bytes,
        ));
        let mut worker = Worker {
            child,
            child_pid,
            _job_guard: job_guard,
            _worker_permit: worker_permit,
            stdin,
            stdout,
            stderr_task,
            described: false,
        };
        let hello = timeout(
            self.config.startup_timeout,
            read_frame::<PythonRunnerWorkerMessage>(&mut worker.stdout),
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
            read_frame::<PythonRunnerWorkerMessage>(&mut worker.stdout),
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

    async fn record_restart(&self) -> Result<(), PythonSupervisorError> {
        let now = Instant::now();
        let mut restarts = self.restarts.lock().await;
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

fn start_error() -> PythonSupervisorError {
    PythonSupervisorError::new(
        "python_worker_start_failed",
        "Python worker could not be started",
    )
}

fn worker_budget(limit: usize) -> Arc<Semaphore> {
    shared_budget(limit, &WORKER_BUDGETS)
}

fn candidate_budget(limit: usize) -> Arc<Semaphore> {
    shared_budget(limit, &CANDIDATE_BUDGETS)
}

type BudgetMap = std::sync::Mutex<std::collections::BTreeMap<usize, Weak<Semaphore>>>;
static WORKER_BUDGETS: OnceLock<BudgetMap> = OnceLock::new();
static CANDIDATE_BUDGETS: OnceLock<BudgetMap> = OnceLock::new();

fn shared_budget(limit: usize, budgets: &'static OnceLock<BudgetMap>) -> Arc<Semaphore> {
    let limit = limit.max(1);
    let mut budgets = budgets
        .get_or_init(|| std::sync::Mutex::new(std::collections::BTreeMap::new()))
        .lock()
        .expect("Python worker budget lock should not be poisoned");
    if let Some(budget) = budgets.get(&limit).and_then(Weak::upgrade) {
        return budget;
    }
    let budget = Arc::new(Semaphore::new(limit));
    budgets.insert(limit, Arc::downgrade(&budget));
    budget
}

struct BusyGuard<'a> {
    busy: &'a AtomicBool,
    discard_worker: &'a AtomicBool,
    completed: bool,
}

impl<'a> BusyGuard<'a> {
    fn new(busy: &'a AtomicBool, discard_worker: &'a AtomicBool) -> Self {
        Self {
            busy,
            discard_worker,
            completed: false,
        }
    }

    fn complete(&mut self) {
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

fn host_call_request_id(call: &crate::python_protocol::PythonRunnerHostCall) -> u64 {
    use crate::python_protocol::PythonRunnerHostCall;
    match call {
        PythonRunnerHostCall::Http { request_id, .. }
        | PythonRunnerHostCall::Secret { request_id, .. }
        | PythonRunnerHostCall::StateGet { request_id, .. }
        | PythonRunnerHostCall::StatePut { request_id, .. }
        | PythonRunnerHostCall::Log { request_id, .. }
        | PythonRunnerHostCall::Metric { request_id, .. }
        | PythonRunnerHostCall::Progress { request_id, .. } => *request_id,
    }
}

fn map_worker_error(code: PythonRunnerErrorCode) -> PythonSupervisorError {
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
        _ => PythonSupervisorError::new(
            "python_provider_failed",
            "Python provider invocation failed",
        ),
    }
}

fn invalid_output() -> PythonSupervisorError {
    PythonSupervisorError::new(
        "python_invalid_output",
        "Python provider produced invalid output",
    )
}

fn protocol_error() -> PythonSupervisorError {
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

async fn write_frame<T: Serialize>(
    writer: &mut OwnedWriteHalf,
    message: &T,
) -> Result<(), PythonSupervisorError> {
    let payload = serde_json::to_vec(message).map_err(|_| invalid_output())?;
    if payload.len() > PYTHON_RUNNER_MAX_FRAME_BYTES {
        return Err(PythonSupervisorError::new(
            "python_input_too_large",
            "Python runner frame exceeded its limit",
        ));
    }
    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

async fn read_frame<T: serde::de::DeserializeOwned>(
    reader: &mut OwnedReadHalf,
) -> Result<T, PythonSupervisorError> {
    let mut header = [0_u8; FRAME_HEADER_BYTES];
    reader.read_exact(&mut header).await?;
    let length = u32::from_be_bytes(header) as usize;
    if length > PYTHON_RUNNER_MAX_FRAME_BYTES {
        return Err(protocol_error());
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).await?;
    serde_json::from_slice(&payload).map_err(|_| protocol_error())
}

async fn drain_stderr<R: AsyncRead + Unpin>(
    mut reader: R,
    retained: Arc<StdMutex<WorkerLogBuffer>>,
    limit: usize,
) {
    let mut buffer = [0_u8; 4096];
    let mut pending = Vec::new();
    loop {
        let Ok(read) = reader.read(&mut buffer).await else {
            return;
        };
        if read == 0 {
            return;
        }
        if limit == 0 {
            continue;
        }
        pending.extend_from_slice(&buffer[..read]);
        while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
            let line = pending.drain(..=newline).collect::<Vec<_>>();
            retain_log_line(&retained, &line, limit);
        }
        if pending.len() > limit {
            let split = pending.len().saturating_sub(limit);
            pending.drain(..split);
        }
    }
}

fn retain_log_line(retained: &StdMutex<WorkerLogBuffer>, line: &[u8], limit: usize) {
    let raw = String::from_utf8_lossy(line)
        .trim_end_matches(['\r', '\n'])
        .to_owned();
    if raw.is_empty() {
        return;
    }
    let message = crate::error::redact_public(&raw);
    let size = message.len();
    let mut retained = retained
        .lock()
        .expect("Python worker log lock should not be poisoned");
    while retained.retained_bytes.saturating_add(size) > limit {
        let Some(removed) = retained.entries.pop_front() else {
            break;
        };
        retained.retained_bytes = retained
            .retained_bytes
            .saturating_sub(removed.message.len());
    }
    if size <= limit {
        let sequence = retained.next_sequence;
        retained.next_sequence = retained.next_sequence.saturating_add(1);
        retained.retained_bytes = retained.retained_bytes.saturating_add(size);
        retained.entries.push_back(PythonWorkerLogEntry {
            sequence,
            stream: "stderr",
            message,
        });
    }
}

async fn terminate_worker(worker: Option<Worker>) {
    let Some(mut worker) = worker else {
        return;
    };
    terminate_process_tree(worker.child_pid);
    let _ = worker.child.kill().await;
    let _ = worker.child.wait().await;
    worker.stderr_task.abort();
}

#[cfg(unix)]
fn terminate_process_tree(pid: Option<u32>) {
    use nix::{sys::signal::Signal, unistd::Pid};
    if let Some(pid) = pid {
        let _ = nix::sys::signal::killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
    }
}

#[cfg(windows)]
fn terminate_process_tree(pid: Option<u32>) {
    let Some(pid) = pid else {
        return;
    };
    // `taskkill /T` is the safe Windows API available to this crate, which
    // forbids all unsafe code. It terminates the worker and its descendants.
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(_pid: Option<u32>) {}

#[derive(Debug, Default)]
struct JobGuard;

impl JobGuard {
    fn new(_pid: Option<u32>) -> Result<Self, PythonSupervisorError> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path};

    fn installed_test_python() -> Option<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packages/python/.venv");
        let path = if cfg!(windows) {
            root.join("Scripts/python.exe")
        } else {
            root.join("bin/python")
        };
        path.is_file().then_some(path)
    }

    fn identity(path: &Path) -> PythonWorkerIdentity {
        let source_digest = Sha256::digest(fs::read(path).expect("provider source"))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        PythonWorkerIdentity {
            path: path.to_owned(),
            generation_id: "supervisor-test-generation".to_owned(),
            source_digest,
            catalog_fingerprint: String::new(),
        }
    }

    #[tokio::test]
    async fn installed_runner_preflights_invokes_times_out_and_restarts() {
        let Some(python) = installed_test_python() else {
            eprintln!("skipping installed runner test: packages/python/.venv is absent");
            return;
        };
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
        let config = PythonSupervisorConfig {
            request_timeout: Duration::from_millis(100),
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
                Duration::from_secs(1),
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
                Duration::from_secs(1),
            )
            .await
            .expect("later invocation restarts without replay");
        assert_eq!(restarted, json!({"value": "restarted"}));
        supervisor.drain_and_shutdown().await;
    }

    #[tokio::test]
    async fn concurrent_invocation_is_rejected_before_queueing() {
        let Some(python) = installed_test_python() else {
            return;
        };
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
                        json!({"delay_ms": 250}),
                        soma_provider_core::ProviderSurface::Mcp,
                        "snapshot-a",
                        Duration::from_secs(1),
                    )
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(30)).await;
        let busy = supervisor
            .invoke(
                "busy-test",
                "wait",
                json!({"delay_ms": 0}),
                soma_provider_core::ProviderSurface::Mcp,
                "snapshot-a",
                Duration::from_secs(1),
            )
            .await
            .expect_err("second invocation must not queue");
        assert_eq!(busy.code(), "python_provider_busy");
        first.await.expect("join").expect("first call");
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn active_invocation_cancels_process_tree_and_later_work_restarts() {
        let Some(python) = installed_test_python() else {
            return;
        };
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
                        json!({"delay_ms": 5_000}),
                        soma_provider_core::ProviderSurface::Mcp,
                        "snapshot-a",
                        Duration::from_secs(10),
                    )
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(supervisor.cancel_active());
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
                Duration::from_secs(1),
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
                Duration::from_secs(1),
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
                Duration::from_secs(1),
            )
            .await
            .expect("rollback activation permits new work");
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn worker_logs_are_bounded_structured_and_redacted() {
        let Some(python) = installed_test_python() else {
            return;
        };
        let temp = tempfile::tempdir().expect("tempdir");
        let provider = temp.path().join("logs.py");
        fs::write(
            &provider,
            r#"
PROVIDER = {"name": "logs-test", "kind": "python"}
def emit() -> dict:
    print("token=super-secret")
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
                .any(|entry| entry.message == "safe diagnostic")
        );
        assert!(
            status
                .logs
                .iter()
                .any(|entry| entry.message == "[redacted provider diagnostic]")
        );
        assert!(
            !serde_json::to_string(&status)
                .unwrap()
                .contains("super-secret")
        );
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

    #[tokio::test]
    async fn quarantine_exhaustion_is_visible_and_operator_reset_recovers() {
        let Some(python) = installed_test_python() else {
            return;
        };
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
            PythonSupervisorConfig {
                request_timeout: Duration::from_millis(200),
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
                Duration::from_secs(1),
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
                Duration::from_secs(1),
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
        let error = read_frame::<PythonRunnerWorkerMessage>(&mut reader)
            .await
            .expect_err("malformed production frame must fail closed");
        assert_eq!(error.code(), "python_protocol_mismatch");
        client.await.expect("client task");
    }
}
