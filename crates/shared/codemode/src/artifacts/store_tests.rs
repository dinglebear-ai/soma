use super::store::ArtifactStore;
use serial_test::serial;

struct EnvVarGuard(Option<std::ffi::OsString>);

impl EnvVarGuard {
    fn set(value: &std::path::Path) -> Self {
        let previous = std::env::var_os("SOMA_HOME");
        // FIXME: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("SOMA_HOME", value) };
        Self(previous)
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.0.take() {
            // FIXME: Audit that the environment access only happens in single-threaded code.
            Some(value) => unsafe { std::env::set_var("SOMA_HOME", value) },
            // FIXME: Audit that the environment access only happens in single-threaded code.
            None => unsafe { std::env::remove_var("SOMA_HOME") },
        }
    }
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn artifact_store_writes_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvVarGuard::set(temp.path());
    let receipt = ArtifactStore::new("run")
        .unwrap()
        .write_text("out.txt", "hello", None)
        .await
        .unwrap();
    assert_eq!(receipt.bytes, 5);
    assert_eq!(receipt.content_type, "text/plain");
}

#[test]
#[serial(code_mode_soma_home)]
fn artifact_store_rejects_unsafe_run_ids() {
    assert!(ArtifactStore::new("../escape").is_err());
    assert!(ArtifactStore::new("/tmp/escape").is_err());
    assert!(ArtifactStore::new("safe-run_01").is_ok());
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn artifact_store_enforces_run_quota() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvVarGuard::set(temp.path());
    let store = ArtifactStore::new("run").unwrap().with_run_limits(5, 1);

    store.write_text("a.txt", "hello", None).await.unwrap();
    let err = store.write_text("b.txt", "x", None).await.unwrap_err();

    assert_eq!(err.kind(), "invalid_param");
}
