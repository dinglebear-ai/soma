use sha2::{Digest, Sha256};
use soma_self_update::{UpdateDirective, UpdateError, UpdateLayout, UpdatePolicy, Updater};
use tempfile::tempdir;
#[cfg(unix)]
use tokio::io::{AsyncRead, ReadBuf};

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tokio::test]
async fn stages_incrementally_and_normalizes_digest() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let updater = Updater::new(
        UpdateLayout::new(&executable, temp.path().join("state.json")),
        UpdatePolicy::default(),
    );
    let body = b"new executable";
    let directive = UpdateDirective::new("2.0.0", "/binary", digest(body).to_uppercase()).unwrap();
    let staged = updater.stage(&body[..], &directive).await.unwrap();
    assert_eq!(staged.bytes_written(), body.len() as u64);
    assert_eq!(staged.sha256(), digest(body));
    assert_eq!(staged.target_version(), "2.0.0");
    assert_eq!(staged.path().parent(), executable.parent());
    assert_eq!(tokio::fs::read(staged.path()).await.unwrap(), body);
}

#[tokio::test]
async fn rejects_oversize_and_digest_mismatch_without_partial_files() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let small = UpdatePolicy::default().with_max_artifact_bytes(3).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, temp.path().join("state.json")),
        small,
    );
    let directive =
        UpdateDirective::new("server-version-data", "/secret-url-data", digest(b"abcd")).unwrap();
    assert!(matches!(
        updater.stage(&b"abcd"[..], &directive).await,
        Err(UpdateError::ArtifactTooLarge { .. })
    ));

    let wrong =
        UpdateDirective::new("server-version-data", "/secret-url-data", digest(b"other")).unwrap();
    assert!(matches!(
        Updater::new(
            UpdateLayout::new(&executable, temp.path().join("state.json")),
            UpdatePolicy::default()
        )
        .stage(&b"abcd"[..], &wrong)
        .await,
        Err(UpdateError::DigestMismatch { .. })
    ));
    let names: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names
            .iter()
            .all(|name| !name.contains("server-version-data") && !name.contains("secret-url-data"))
    );
    assert!(names.is_empty(), "partial artifacts remain: {names:?}");
}

#[tokio::test]
async fn staging_paths_are_unique() {
    let temp = tempdir().unwrap();
    let updater = Updater::new(
        UpdateLayout::new(temp.path().join("example"), temp.path().join("state.json")),
        UpdatePolicy::default(),
    );
    let directive = UpdateDirective::new("2", "/binary", digest(b"x")).unwrap();
    let first = updater.stage(&b"x"[..], &directive).await.unwrap();
    let second = updater.stage(&b"x"[..], &directive).await.unwrap();
    assert_ne!(first.path(), second.path());
}

#[cfg(unix)]
#[tokio::test]
async fn staging_preserves_existing_executable_mode() {
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o700, 0o750] {
        let temp = tempdir().unwrap();
        let executable = temp.path().join("example");
        std::fs::write(&executable, b"old").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(mode)).unwrap();
        let updater = Updater::new(
            UpdateLayout::new(&executable, temp.path().join("state.json")),
            UpdatePolicy::default(),
        );
        let directive = UpdateDirective::new("2", "/binary", digest(b"new")).unwrap();
        let staged = updater.stage(&b"new"[..], &directive).await.unwrap();
        assert_eq!(
            std::fs::metadata(staged.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            mode
        );
    }
}

#[tokio::test]
async fn explicit_staged_cleanup_reports_the_affected_path() {
    let temp = tempdir().unwrap();
    let updater = Updater::new(
        UpdateLayout::new(temp.path().join("example"), temp.path().join("state.json")),
        UpdatePolicy::default(),
    );
    let directive = UpdateDirective::new("2", "/binary", digest(b"new")).unwrap();
    let staged = updater.stage(&b"new"[..], &directive).await.unwrap();
    let path = staged.path().to_path_buf();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("child"), b"keep directory non-empty").unwrap();
    match staged.cleanup().unwrap_err() {
        UpdateError::Io { path: failed, .. } => assert_eq!(failed, path),
        error => panic!("unexpected cleanup error: {error}"),
    }
}

#[cfg(unix)]
struct CleanupSabotageReader {
    directory: std::path::PathBuf,
    sabotaged: bool,
}

#[cfg(unix)]
impl AsyncRead for CleanupSabotageReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if !self.sabotaged {
            let partial = std::fs::read_dir(&self.directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.to_string_lossy().ends_with(".part"))
                .unwrap();
            std::fs::remove_file(&partial).unwrap();
            std::fs::create_dir(&partial).unwrap();
            std::fs::write(partial.join("child"), b"prevent directory removal").unwrap();
            self.sabotaged = true;
        }
        std::task::Poll::Ready(Err(std::io::Error::other("download failed")))
    }
}

#[cfg(unix)]
#[tokio::test]
async fn failed_staging_reports_both_operation_and_cleanup_errors() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let updater = Updater::new(
        UpdateLayout::new(&executable, temp.path().join("state.json")),
        UpdatePolicy::default(),
    );
    let directive = UpdateDirective::new("2", "/binary", digest(b"new")).unwrap();
    let error = updater
        .stage(
            CleanupSabotageReader {
                directory: temp.path().to_path_buf(),
                sabotaged: false,
            },
            &directive,
        )
        .await
        .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("download failed"), "{message}");
    assert!(message.contains("cleanup"), "{message}");
}
