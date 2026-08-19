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

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn artifact_store_prunes_only_when_first_write_occurs() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvVarGuard::set(temp.path());
    let root = temp.path().join("code-mode-artifacts");
    tokio::fs::create_dir_all(root.join("old-one"))
        .await
        .unwrap();
    tokio::fs::write(root.join("old-one/payload"), b"old")
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    tokio::fs::create_dir_all(root.join("old-two"))
        .await
        .unwrap();
    tokio::fs::write(root.join("old-two/payload"), b"old")
        .await
        .unwrap();

    let store = ArtifactStore::new("current")
        .unwrap()
        .with_retention_limits(1, 0);
    assert!(root.join("old-one").exists());
    assert!(root.join("old-two").exists());

    store.write_text("out.txt", "new", None).await.unwrap();
    assert!(!root.join("old-one").exists());
    assert!(root.join("old-two").exists());
    assert!(root.join("current/out.txt").exists());
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn active_peer_store_survives_another_runs_prune() {
    let temp = tempfile::tempdir().unwrap();
    let _home = EnvVarGuard::set(temp.path());
    let root = temp.path().join("code-mode-artifacts");

    let peer = ArtifactStore::new("peer")
        .unwrap()
        .with_retention_limits(0, 0);
    peer.write_text("peer.txt", "peer", None).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let current = ArtifactStore::new("current")
        .unwrap()
        .with_retention_limits(1, 1);
    current
        .write_text("current.txt", "current", None)
        .await
        .unwrap();

    assert!(root.join("peer/peer.txt").exists());
    assert!(root.join("current/current.txt").exists());
}
