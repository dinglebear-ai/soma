use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    net::IpAddr,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::time::timeout;
use wasmtime::Engine;

use super::WasmArtifact;
use super::WasmStoreState;
use crate::wasm_memory::public_ip;

struct Admission {
    active: Mutex<AdmissionState>,
    ready: Condvar,
    count_limit: usize,
    weight_limit: usize,
}

#[derive(Default)]
struct AdmissionState {
    count: usize,
    weight: usize,
}

pub(super) struct AdmissionPermit {
    admission: &'static Admission,
    weight: usize,
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.admission.active.lock() {
            active.count = active.count.saturating_sub(1);
            active.weight = active.weight.saturating_sub(self.weight);
            self.admission.ready.notify_all();
        }
    }
}

fn acquire(
    admission: &'static Admission,
    deadline: Instant,
    label: &str,
    weight: usize,
) -> Result<AdmissionPermit, String> {
    let mut active = admission
        .active
        .lock()
        .map_err(|_| format!("WASM {label} admission lock is poisoned"))?;
    if weight > admission.weight_limit {
        return Err(format!(
            "WASM {label} request exceeds global resource admission"
        ));
    }
    while active.count >= admission.count_limit
        || active.weight.saturating_add(weight) > admission.weight_limit
    {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                format!("WASM invocation deadline expired waiting for {label} admission")
            })?;
        let (next, wait) = admission
            .ready
            .wait_timeout(active, remaining)
            .map_err(|_| format!("WASM {label} admission lock is poisoned"))?;
        active = next;
        if wait.timed_out() {
            return Err(format!(
                "WASM invocation deadline expired waiting for {label} admission"
            ));
        }
    }
    active.count += 1;
    active.weight += weight;
    Ok(AdmissionPermit { admission, weight })
}

pub(super) fn acquire_compile(deadline: Instant) -> Result<AdmissionPermit, String> {
    static ADMISSION: OnceLock<Admission> = OnceLock::new();
    acquire(
        ADMISSION.get_or_init(|| Admission {
            active: Mutex::new(AdmissionState::default()),
            ready: Condvar::new(),
            count_limit: 2,
            weight_limit: 2,
        }),
        deadline,
        "compilation",
        1,
    )
}

pub(super) fn acquire_execution(
    deadline: Instant,
    max_memory_bytes: usize,
) -> Result<AdmissionPermit, String> {
    static ADMISSION: OnceLock<Admission> = OnceLock::new();
    const MAX_MEMORIES_PER_STORE: usize = 4;
    const MIN_EXECUTION_WEIGHT: usize = 32 * 1024 * 1024;
    const AGGREGATE_MEMORY_LIMIT: usize = 1024 * 1024 * 1024;
    let weight = max_memory_bytes
        .saturating_mul(MAX_MEMORIES_PER_STORE)
        .max(MIN_EXECUTION_WEIGHT);
    acquire(
        ADMISSION.get_or_init(|| Admission {
            active: Mutex::new(AdmissionState::default()),
            ready: Condvar::new(),
            count_limit: 32,
            weight_limit: AGGREGATE_MEMORY_LIMIT,
        }),
        deadline,
        "execution",
        weight,
    )
}

pub(super) async fn resolve_component_hosts(
    capabilities: &soma_provider_core::HostCapabilities,
    deadline: Instant,
) -> Result<BTreeMap<String, Vec<IpAddr>>, String> {
    let Some(network) = capabilities
        .network
        .as_ref()
        .filter(|network| network.enabled)
    else {
        return Ok(BTreeMap::new());
    };
    const MAX_HOSTS: usize = 32;
    let hosts = network
        .allowed_hosts
        .iter()
        .map(|host| host.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    if hosts.len() > MAX_HOSTS {
        return Err(format!(
            "component network capability exceeds {MAX_HOSTS} unique hosts"
        ));
    }
    if hosts.iter().any(String::is_empty) {
        return Err("component network capability contains an empty host".to_owned());
    }
    let resolve = async move {
        let mut tasks = tokio::task::JoinSet::new();
        for hostname in hosts {
            tasks.spawn(async move {
                let addresses = tokio::net::lookup_host((hostname.as_str(), 443))
                    .await
                    .map_err(|_| format!("HTTP host resolution failed for `{hostname}`"))?
                    .map(|address| address.ip())
                    .collect::<Vec<_>>();
                Ok::<_, String>((hostname, addresses))
            });
        }
        let mut resolved = BTreeMap::new();
        while let Some(result) = tasks.join_next().await {
            let (hostname, addresses) =
                result.map_err(|error| format!("HTTP host resolution task failed: {error}"))??;
            if addresses.is_empty() || addresses.iter().any(|address| !public_ip(*address)) {
                return Err(format!(
                    "HTTP host `{hostname}` resolved to a non-public address"
                ));
            }
            resolved.insert(hostname, addresses);
        }
        Ok(resolved)
    };
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .unwrap_or(Duration::ZERO);
    timeout(remaining, resolve)
        .await
        .map_err(|_| "component invocation deadline expired during DNS resolution".to_owned())?
}

pub(super) fn component_broker(
    state: &WasmStoreState,
) -> Result<&soma_provider_core::BrokerCapability, String> {
    state
        .capabilities
        .broker
        .as_ref()
        .filter(|capability| capability.enabled)
        .ok_or_else(|| "broker capability not declared".to_owned())
}

pub(super) fn component_require_scope(state: &WasmStoreState, write: bool) -> Result<(), String> {
    if state.context.actor_id.as_deref().is_none_or(str::is_empty) {
        return Err("authenticated actor context is required".to_owned());
    }
    let allowed = state
        .context
        .actor_scopes
        .iter()
        .any(|scope| scope == "soma:write" || (!write && scope == "soma:read"));
    allowed
        .then_some(())
        .ok_or_else(|| "actor scopes do not authorize this host operation".to_owned())
}

pub(super) fn component_remaining(state: &WasmStoreState) -> Result<Duration, String> {
    state
        .deadline
        .checked_duration_since(std::time::Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| "component invocation deadline expired".to_owned())
}

pub(super) fn component_forbidden_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "content-length"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

pub(super) fn component_metric(
    state: &WasmStoreState,
    name: &str,
    value: f64,
    attributes: &str,
) -> Result<(), String> {
    component_require_scope(state, false)?;
    if !component_broker(state)?.metrics {
        return Err("metrics capability is not declared".to_owned());
    }
    if !value.is_finite() {
        return Err("metric value must be finite".to_owned());
    }
    tracing::info!(
        metric = %name.chars().take(128).collect::<String>(),
        value,
        attributes = %super::component_diagnostic(state, attributes),
        "component provider metric"
    );
    Ok(())
}

pub(super) fn component_progress(
    state: &WasmStoreState,
    current: u64,
    total: Option<u64>,
    message: Option<&str>,
) -> Result<(), String> {
    component_require_scope(state, false)?;
    if !component_broker(state)?.progress {
        return Err("progress capability is not declared".to_owned());
    }
    tracing::info!(
        current,
        ?total,
        message = %super::component_diagnostic(state, message.unwrap_or_default()),
        "component provider progress"
    );
    state.context.progress.report(current, total, message);
    Ok(())
}

enum ArtifactCellState {
    Empty,
    Compiling,
    Ready(Arc<WasmArtifact>),
}

pub(super) struct ArtifactCell {
    state: Mutex<ArtifactCellState>,
    ready: Condvar,
}

impl ArtifactCell {
    fn new() -> Self {
        Self {
            state: Mutex::new(ArtifactCellState::Empty),
            ready: Condvar::new(),
        }
    }

    fn is_compiling(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| matches!(*state, ArtifactCellState::Compiling))
    }

    pub(super) fn get_or_compile(
        &self,
        deadline: Instant,
        compile: impl FnOnce() -> Result<Arc<WasmArtifact>, String>,
    ) -> Result<Arc<WasmArtifact>, String> {
        let mut compile = Some(compile);
        loop {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "WASM artifact cell lock is poisoned".to_owned())?;
            match &*state {
                ArtifactCellState::Ready(artifact) => return Ok(artifact.clone()),
                ArtifactCellState::Empty => {
                    if Instant::now() >= deadline {
                        return Err(
                            "WASM invocation deadline expired before compilation".to_owned()
                        );
                    }
                    *state = ArtifactCellState::Compiling;
                    drop(state);
                    let result = compile
                        .take()
                        .expect("only the compiling owner consumes the closure")(
                    );
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| "WASM artifact cell lock is poisoned".to_owned())?;
                    *state = match &result {
                        Ok(artifact) => ArtifactCellState::Ready(artifact.clone()),
                        Err(_) => ArtifactCellState::Empty,
                    };
                    self.ready.notify_all();
                    return result;
                }
                ArtifactCellState::Compiling => {
                    let remaining =
                        deadline
                            .checked_duration_since(Instant::now())
                            .ok_or_else(|| {
                                "WASM invocation deadline expired while waiting for compilation"
                                    .to_owned()
                            })?;
                    let (_state, wait) = self
                        .ready
                        .wait_timeout(state, remaining)
                        .map_err(|_| "WASM artifact cell lock is poisoned".to_owned())?;
                    if wait.timed_out() {
                        return Err(
                            "WASM invocation deadline expired while waiting for compilation"
                                .to_owned(),
                        );
                    }
                }
            }
        }
    }
}

type SharedArtifactCell = Arc<ArtifactCell>;

#[derive(Default)]
pub(super) struct WasmArtifactCache {
    values: BTreeMap<String, SharedArtifactCell>,
    weights: BTreeMap<String, usize>,
    order: VecDeque<String>,
    total_bytes: usize,
}

impl WasmArtifactCache {
    pub(super) const MAX_ARTIFACTS: usize = 32;
    const MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;

    pub(super) fn cell(&mut self, digest: String, bytes: usize) -> SharedArtifactCell {
        if let Some(value) = self.values.get(&digest).cloned() {
            self.order.retain(|entry| entry != &digest);
            self.order.push_back(digest);
            return value;
        }
        let value = Arc::new(ArtifactCell::new());
        self.values.insert(digest.clone(), value.clone());
        self.weights.insert(digest.clone(), bytes);
        self.total_bytes = self.total_bytes.saturating_add(bytes);
        self.order.retain(|entry| entry != &digest);
        self.order.push_back(digest);
        value
    }

    pub(super) fn prune(&mut self) {
        let mut inspected = 0;
        while self.values.len() > Self::MAX_ARTIFACTS || self.total_bytes > Self::MAX_TOTAL_BYTES {
            if let Some(oldest) = self.order.pop_front() {
                if self
                    .values
                    .get(&oldest)
                    .is_some_and(|cell| cell.is_compiling())
                {
                    self.order.push_back(oldest);
                    inspected += 1;
                    if inspected >= self.order.len() {
                        break;
                    }
                    continue;
                }
                self.values.remove(&oldest);
                self.total_bytes = self
                    .total_bytes
                    .saturating_sub(self.weights.remove(&oldest).unwrap_or_default());
                inspected = 0;
            }
        }
    }
}

pub(super) struct EpochTicker {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl EpochTicker {
    pub(super) fn start(engine: Engine) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let handle = std::thread::Builder::new()
            .name("soma-wasm-epoch".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(10));
                    engine.increment_epoch();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use wasmtime::Module;

    #[test]
    fn admission_wait_is_bounded_by_the_invocation_deadline() {
        let admission = Box::leak(Box::new(Admission {
            active: Mutex::new(AdmissionState::default()),
            ready: Condvar::new(),
            count_limit: 1,
            weight_limit: 1,
        }));
        let _permit = acquire(
            admission,
            Instant::now() + Duration::from_secs(1),
            "test",
            1,
        )
        .expect("permit");
        let error = acquire(
            admission,
            Instant::now() + Duration::from_millis(5),
            "test",
            1,
        )
        .err()
        .expect("second permit must time out");
        assert!(error.contains("deadline"));
    }

    #[test]
    fn weighted_admission_rejects_aggregate_pressure_and_releases_weight() {
        let admission = Box::leak(Box::new(Admission {
            active: Mutex::new(AdmissionState::default()),
            ready: Condvar::new(),
            count_limit: 8,
            weight_limit: 10,
        }));
        let first = acquire(
            admission,
            Instant::now() + Duration::from_secs(1),
            "test",
            8,
        )
        .expect("first weighted permit");
        let error = acquire(
            admission,
            Instant::now() + Duration::from_millis(5),
            "test",
            3,
        )
        .err()
        .expect("aggregate weight must be bounded");
        assert!(error.contains("deadline"));
        drop(first);
        acquire(
            admission,
            Instant::now() + Duration::from_secs(1),
            "test",
            3,
        )
        .expect("released weight is reusable");
    }

    #[test]
    fn cache_prunes_ready_cells_after_overlapping_compilation_pressure() {
        let mut cache = WasmArtifactCache::default();
        let first = cache.cell("first".to_owned(), 400 * 1024 * 1024);
        assert!(
            cache.values.contains_key("first"),
            "a newly admitted cell must remain reachable until compilation starts"
        );
        *first.state.lock().expect("first cell") = ArtifactCellState::Compiling;
        let second = cache.cell("second".to_owned(), 400 * 1024 * 1024);
        assert!(
            cache.values.contains_key("second"),
            "the new cell must not evict itself before it is marked compiling"
        );
        *second.state.lock().expect("second cell") = ArtifactCellState::Compiling;
        {
            *first.state.lock().expect("first cell") =
                ArtifactCellState::Ready(Arc::new(WasmArtifact::Core(
                    Module::new(&Engine::default(), b"\0asm\x01\0\0\0").expect("empty module"),
                )));
            *second.state.lock().expect("second cell") =
                ArtifactCellState::Ready(Arc::new(WasmArtifact::Core(
                    Module::new(&Engine::default(), b"\0asm\x01\0\0\0").expect("empty module"),
                )));
        }
        cache.prune();
        assert!(cache.total_bytes <= WasmArtifactCache::MAX_TOTAL_BYTES);
        assert_eq!(cache.values.len(), 1);
    }
}
