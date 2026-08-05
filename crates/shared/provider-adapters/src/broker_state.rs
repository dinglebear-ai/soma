//! Durable, quota-enforced state shared by isolated provider runtimes.

use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, TryLockError, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_KEYS: usize = 4_096;
const MAX_NAMESPACE_KEYS: usize = 1_024;
const MAX_KEY_BYTES: usize = 1_024;
const MAX_VALUE_BYTES: usize = 64 * 1024;
const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
const MAX_NAMESPACE_BYTES: usize = 1024 * 1024;
const STATE_SCHEMA_VERSION: u32 = 1;

type StoreRegistry = Mutex<BTreeMap<PathBuf, Weak<BrokerStateStore>>>;
static STORES: OnceLock<StoreRegistry> = OnceLock::new();
static CONFIGURED_PATH: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn configure(path: PathBuf) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("provider state path must be absolute".to_owned());
    }
    if let Some(existing) = CONFIGURED_PATH.get() {
        return (existing == &path)
            .then_some(())
            .ok_or_else(|| "provider state path was already configured differently".to_owned());
    }
    CONFIGURED_PATH
        .set(path)
        .map_err(|_| "provider state path could not be configured".to_owned())
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StateDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema_version: Option<u32>,
    #[serde(default)]
    values: BTreeMap<String, Value>,
}

/// One process-shared state store whose updates are persisted atomically.
#[derive(Debug)]
pub(crate) struct BrokerStateStore {
    path: PathBuf,
    values: Mutex<BTreeMap<String, Value>>,
}

impl BrokerStateStore {
    pub(crate) fn configured() -> Result<Arc<Self>, String> {
        let path = configured_path()?;
        let mut stores = STORES
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .map_err(|_| "provider state registry lock is poisoned".to_owned())?;
        stores.retain(|_, store| store.strong_count() != 0);
        if let Some(store) = stores.get(&path).and_then(Weak::upgrade) {
            return Ok(store);
        }
        let store = Arc::new(Self::open(path.clone())?);
        stores.insert(path, Arc::downgrade(&store));
        Ok(store)
    }

    fn open(path: PathBuf) -> Result<Self, String> {
        let values = match read_state_bytes(&path) {
            Ok(bytes) => {
                let document: StateDocument = serde_json::from_slice(&bytes)
                    .map_err(|_| "provider state file is invalid".to_owned())?;
                validate_schema(&document)?;
                validate_document(&document.values)?;
                document.values
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(_) => return Err("provider state file is unreadable".to_owned()),
        };
        Ok(Self {
            path,
            values: Mutex::new(values),
        })
    }

    pub(crate) fn get(
        &self,
        namespace: &str,
        key: &str,
        deadline: Instant,
        cancelled: Option<&AtomicBool>,
    ) -> Result<Value, String> {
        validate_name("state namespace", namespace)?;
        validate_name("state key", key)?;
        let mut values = mutex_before(&self.values, deadline, cancelled)?;
        let file_lock = state_file_lock(&self.path, false, deadline, cancelled)?;
        if !self.path.as_os_str().is_empty() {
            *values = read_values(&self.path)?;
        }
        let result = values
            .get(&namespaced_key(namespace, key))
            .cloned()
            .unwrap_or(Value::Null);
        drop(file_lock);
        Ok(result)
    }

    pub(crate) fn put(
        &self,
        namespace: &str,
        key: &str,
        value: &Value,
        deadline: Instant,
        cancelled: Option<&AtomicBool>,
    ) -> Result<(), String> {
        validate_name("state namespace", namespace)?;
        validate_name("state key", key)?;
        let encoded =
            serde_json::to_vec(value).map_err(|_| "state value is not valid JSON".to_owned())?;
        if encoded.len() > MAX_VALUE_BYTES {
            return Err("state value exceeds provider state limit".to_owned());
        }
        let mut values = mutex_before(&self.values, deadline, cancelled)?;
        let file_lock = state_file_lock(&self.path, true, deadline, cancelled)?;
        let mut candidate = if self.path.as_os_str().is_empty() {
            values.clone()
        } else {
            read_values(&self.path)?
        };
        let storage_key = namespaced_key(namespace, key);
        if value.is_null() {
            candidate.remove(&storage_key);
        } else {
            candidate.insert(storage_key, value.clone());
        }
        validate_document(&candidate)?;
        persist(&self.path, &candidate)?;
        *values = candidate;
        drop(file_lock);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn in_memory_for_test() -> Arc<Self> {
        Arc::new(Self {
            path: PathBuf::new(),
            values: Mutex::new(BTreeMap::new()),
        })
    }
}

fn state_file_lock(
    path: &Path,
    exclusive: bool,
    deadline: Instant,
    cancelled: Option<&AtomicBool>,
) -> Result<Option<fs::File>, String> {
    if path.as_os_str().is_empty() {
        return Ok(None);
    }
    let parent = path
        .parent()
        .ok_or_else(|| "provider state path has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|_| "provider state directory could not be created".to_owned())?;
    secure_directory(parent)?;
    let lock_path = path.with_extension("json.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|_| "provider state lock file could not be opened".to_owned())?;
    loop {
        check_wait(deadline, cancelled)?;
        let locked = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        // Fully qualified: `std::sync::TryLockError` is already in scope here
        // for the mutex helpers below and is a different type.
        match locked {
            Ok(()) => return Ok(Some(file)),
            Err(std::fs::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(std::fs::TryLockError::Error(_)) => {
                return Err("provider state file could not be locked".to_owned());
            }
        }
    }
}

fn mutex_before<'a, T>(
    mutex: &'a Mutex<T>,
    deadline: Instant,
    cancelled: Option<&AtomicBool>,
) -> Result<MutexGuard<'a, T>, String> {
    loop {
        check_wait(deadline, cancelled)?;
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            Err(TryLockError::Poisoned(_)) => {
                return Err("provider state lock is poisoned".to_owned());
            }
        }
    }
}

fn check_wait(deadline: Instant, cancelled: Option<&AtomicBool>) -> Result<(), String> {
    if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err("provider state operation was cancelled".to_owned());
    }
    if Instant::now() >= deadline {
        return Err("provider state operation deadline expired".to_owned());
    }
    Ok(())
}

fn read_values(path: &Path) -> Result<BTreeMap<String, Value>, String> {
    match read_state_bytes(path) {
        Ok(bytes) => {
            let document: StateDocument = serde_json::from_slice(&bytes)
                .map_err(|_| "provider state file is invalid".to_owned())?;
            validate_schema(&document)?;
            validate_document(&document.values)?;
            Ok(document.values)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(_) => Err("provider state file is unreadable".to_owned()),
    }
}

fn read_state_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_TOTAL_BYTES as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "provider state file exceeds total state limit",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TOTAL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TOTAL_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "provider state file exceeds total state limit",
        ));
    }
    Ok(bytes)
}

fn configured_path() -> Result<PathBuf, String> {
    if let Some(path) = CONFIGURED_PATH.get() {
        return Ok(path.clone());
    }
    let state_root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })
        .ok_or_else(|| {
            "SOMA_PROVIDER_STATE_PATH or an absolute user state directory is required".to_owned()
        })?;
    if !state_root.is_absolute() {
        return Err("provider state directory must be absolute".to_owned());
    }
    Ok(state_root.join("soma").join("provider-state.json"))
}

fn validate_name(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_KEY_BYTES || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn namespaced_key(namespace: &str, key: &str) -> String {
    format!("{namespace}\0{key}")
}

fn validate_document(values: &BTreeMap<String, Value>) -> Result<(), String> {
    if values.len() > MAX_KEYS {
        return Err("provider state key quota exceeded".to_owned());
    }
    let encoded = serde_json::to_vec(&StateDocument {
        schema_version: Some(STATE_SCHEMA_VERSION),
        values: values.clone(),
    })
    .map_err(|_| "provider state could not be serialized".to_owned())?;
    if encoded.len() > MAX_TOTAL_BYTES {
        return Err("provider state aggregate quota exceeded".to_owned());
    }
    let mut namespace_usage = BTreeMap::<&str, (usize, usize)>::new();
    for (key, value) in values {
        let mut parts = key.split('\0');
        let namespace = parts
            .next()
            .ok_or_else(|| "provider state key is invalid".to_owned())?;
        let logical_key = parts
            .next()
            .ok_or_else(|| "provider state key is invalid".to_owned())?;
        if parts.next().is_some()
            || validate_name("state namespace", namespace).is_err()
            || validate_name("state key", logical_key).is_err()
        {
            return Err("provider state key is invalid".to_owned());
        }
        let value_bytes =
            serde_json::to_vec(value).map_err(|_| "provider state value is invalid".to_owned())?;
        if key.len() > MAX_KEY_BYTES * 2 + 1 || value_bytes.len() > MAX_VALUE_BYTES {
            return Err("provider state entry exceeds quota".to_owned());
        }
        let usage = namespace_usage.entry(namespace).or_default();
        usage.0 += 1;
        usage.1 += key.len() + value_bytes.len();
        if usage.0 > MAX_NAMESPACE_KEYS || usage.1 > MAX_NAMESPACE_BYTES {
            return Err("provider state namespace quota exceeded".to_owned());
        }
    }
    Ok(())
}

fn persist(path: &Path, values: &BTreeMap<String, Value>) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "provider state path has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|_| "provider state directory could not be created".to_owned())?;
    let body = serde_json::to_vec(&StateDocument {
        schema_version: Some(STATE_SCHEMA_VERSION),
        values: values.clone(),
    })
    .map_err(|_| "provider state could not be serialized".to_owned())?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            use std::io::Write as _;

            file.write_all(&body)?;
            file.sync_all()
        })
        .map_err(|_| "provider state file could not be atomically published".to_owned())?;
    secure_file(path)?;
    sync_parent(parent)
}

#[cfg(unix)]
fn secure_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "provider state directory permissions could not be secured".to_owned())
}

#[cfg(not(unix))]
fn secure_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn secure_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "provider state file permissions could not be secured".to_owned())
}

#[cfg(not(unix))]
fn secure_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn validate_schema(document: &StateDocument) -> Result<(), String> {
    if document
        .schema_version
        .is_some_and(|version| version != STATE_SCHEMA_VERSION)
    {
        return Err("provider state schema version is unsupported".to_owned());
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "provider state directory could not be synced".to_owned())
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Budget for state operations these tests expect to *succeed*.
    ///
    /// Not the property under test — it only stops a genuinely stuck lock from
    /// hanging the suite. It has to cover the slowest legitimate case
    /// (`independent_store_instances_serialize_durable_updates` performs 50
    /// lock-contended, fsync'd writes), so a tight value here just converts
    /// disk and scheduler pressure into a false failure.
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    #[test]
    fn state_is_namespaced_shared_and_quota_enforced() {
        let store = BrokerStateStore::in_memory_for_test();
        store
            .put("one", "key", &Value::from(1), deadline(), None)
            .unwrap();
        assert_eq!(
            store.get("one", "key", deadline(), None).unwrap(),
            Value::from(1)
        );
        assert_eq!(
            store.get("two", "key", deadline(), None).unwrap(),
            Value::Null
        );
        store
            .put("one", "key", &Value::Null, deadline(), None)
            .unwrap();
        assert_eq!(
            store.get("one", "key", deadline(), None).unwrap(),
            Value::Null
        );
        assert!(
            store
                .put(
                    "one",
                    "huge",
                    &Value::String("x".repeat(MAX_VALUE_BYTES)),
                    deadline(),
                    None,
                )
                .is_err()
        );
    }

    #[test]
    fn independent_store_instances_serialize_durable_updates() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("state.json");
        let first = Arc::new(BrokerStateStore::open(path.clone()).expect("first store"));
        let second = Arc::new(BrokerStateStore::open(path).expect("second store"));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let threads = [first, second]
            .into_iter()
            .enumerate()
            .map(|(writer, store)| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    for index in 0..25 {
                        store
                            .put(
                                "shared",
                                &format!("{writer}-{index}"),
                                &Value::from(index),
                                deadline(),
                                None,
                            )
                            .expect("durable write");
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().expect("writer thread");
        }
        let values = read_values(&temp.path().join("state.json")).expect("persisted document");
        assert_eq!(values.len(), 50);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(temp.path().join("state.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn state_lock_wait_honors_deadline_and_cancellation() {
        let store = BrokerStateStore::in_memory_for_test();
        let _held = store.values.lock().expect("hold state lock");
        assert!(
            store
                .get(
                    "one",
                    "key",
                    Instant::now() + Duration::from_millis(5),
                    None,
                )
                .expect_err("deadline must stop lock wait")
                .contains("deadline")
        );
        let cancelled = AtomicBool::new(true);
        assert!(
            store
                .get("one", "key", deadline(), Some(&cancelled))
                .expect_err("cancellation must stop lock wait")
                .contains("cancelled")
        );
    }

    #[test]
    fn oversized_state_file_is_rejected_before_parsing() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("state.json");
        let file = fs::File::create(&path).expect("state file");
        file.set_len(MAX_TOTAL_BYTES as u64 + 1)
            .expect("oversized state file");

        assert!(
            BrokerStateStore::open(path)
                .expect_err("oversized file must be rejected")
                .contains("unreadable")
        );
    }
}
