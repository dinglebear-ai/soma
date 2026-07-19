use sha2::{Digest, Sha256};
#[cfg(unix)]
use soma_self_update::{ConfirmationOutcome, RecoveryAction};
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
    assert_eq!(
        staged.path().parent().unwrap().canonicalize().unwrap(),
        executable.parent().unwrap().canonicalize().unwrap()
    );
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
#[test]
fn bare_relative_layout_stays_bound_to_construction_directory() {
    use std::os::unix::fs::PermissionsExt;

    const CHILD_ENV: &str = "SOMA_SELF_UPDATE_RELATIVE_LAYOUT_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        let old = b"#!/bin/sh\necho 'agent 1'\n";
        let new = b"#!/bin/sh\necho 'agent 2'\n";
        let root = std::env::current_dir().unwrap();
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(second.join("agent"), b"second directory sentinel").unwrap();
        std::env::set_current_dir(&first).unwrap();
        std::fs::write("agent", old).unwrap();
        std::fs::set_permissions("agent", std::fs::Permissions::from_mode(0o700)).unwrap();
        let updater = Updater::new(
            UpdateLayout::new("agent", "state.json"),
            UpdatePolicy::default(),
        );
        assert_eq!(updater.layout().executable(), first.join("agent"));
        assert_eq!(updater.layout().state_file(), first.join("state.json"));
        let directive = UpdateDirective::new("2", "/agent", digest(new)).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let staged = updater.stage(&new[..], &directive).await.unwrap();
            assert_eq!(
                staged.path().parent().unwrap().canonicalize().unwrap(),
                first.canonicalize().unwrap()
            );
            let validated = updater.validate(staged).await.unwrap();
            std::env::set_current_dir(&second).unwrap();
            updater.install(validated, "1").await.unwrap();
            assert!(matches!(
                updater.recover_on_startup("2").await.unwrap(),
                RecoveryAction::PendingUpdate { .. }
            ));
            assert_eq!(
                updater.confirm_success("2").await.unwrap(),
                ConfirmationOutcome::Confirmed {
                    version: "2".into()
                }
            );
        });
        assert_eq!(std::fs::read(first.join("agent")).unwrap(), new);
        assert!(!first.join("state.json").exists());
        assert_eq!(
            std::fs::read(second.join("agent")).unwrap(),
            b"second directory sentinel"
        );
        assert!(!second.join("state.json").exists());
        return;
    }

    let temp = tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "bare_relative_layout_stays_bound_to_construction_directory",
            "--nocapture",
        ])
        .current_dir(temp.path())
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "relative-layout child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn staged_artifact_is_private_until_verified_under_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;

    const CHILD_ENV: &str = "SOMA_SELF_UPDATE_PRIVATE_STAGE_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        let _previous = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
        let payload = b"#!/bin/sh\necho 'agent 2'\n";
        let executable = std::env::current_dir().unwrap().join("agent");
        std::fs::write(&executable, b"#!/bin/sh\necho 'agent 1'\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o750)).unwrap();
        let updater = Updater::new(
            UpdateLayout::new(&executable, "state.json"),
            UpdatePolicy::default(),
        );
        let directive = UpdateDirective::new("2", "/agent", digest(payload)).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let (mut writer, reader) = tokio::io::duplex(64);
            let stage = tokio::spawn(async move { updater.stage(reader, &directive).await });
            writer.write_all(payload).await.unwrap();
            writer.flush().await.unwrap();
            let mut partial = None;
            for _ in 0..1_000 {
                if let Some(path) = std::fs::read_dir(executable.parent().unwrap())
                    .unwrap()
                    .filter_map(std::result::Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.file_name()
                            .is_some_and(|name| name.to_string_lossy().contains(".update-"))
                    })
                {
                    partial = Some(path);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            let partial = partial.expect("staging task never created its private partial file");
            assert_eq!(
                std::fs::metadata(&partial).unwrap().permissions().mode() & 0o777,
                0o600
            );
            drop(writer);
            let staged = stage.await.unwrap().unwrap();
            assert_eq!(
                std::fs::metadata(staged.path())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        });
        return;
    }

    let temp = tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "staged_artifact_is_private_until_verified_under_permissive_umask",
            "--nocapture",
        ])
        .current_dir(temp.path())
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "private-stage child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn staging_uses_private_validation_mode_for_supported_source_modes() {
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o700, 0o750, 0o755] {
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
                & 0o7777,
            0o700
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn unsafe_source_modes_are_rejected_before_reading_or_creating_transaction_artifacts() {
    use std::os::unix::fs::PermissionsExt;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    struct PanicReader;

    impl AsyncRead for PanicReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _buffer: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            panic!("unsafe executable mode must be rejected before reading artifact bytes");
        }
    }

    for mode in [0o4755, 0o2755, 0o1755, 0o777, 0o775, 0o600, 0o644] {
        let temp = tempdir().unwrap();
        let executable = temp.path().join("example");
        let state = temp.path().join("state.json");
        std::fs::write(&executable, b"old").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(mode)).unwrap();
        let updater = Updater::new(
            UpdateLayout::new(&executable, &state),
            UpdatePolicy::default(),
        );
        let directive = UpdateDirective::new("2", "/binary", digest(b"new")).unwrap();

        assert!(matches!(
            updater.preflight_stage(),
            Err(UpdateError::UnsafeExecutableMode {
                mode: rejected_mode,
                ..
            }) if rejected_mode == mode
        ));
        match updater.stage(PanicReader, &directive).await {
            Err(UpdateError::UnsafeExecutableMode {
                path,
                mode: rejected_mode,
                remediation,
            }) => {
                assert_eq!(path, executable);
                assert_eq!(rejected_mode, mode);
                assert!(remediation.contains("grant owner execute"));
            }
            result => panic!("expected typed unsafe-mode rejection, got {result:?}"),
        }

        assert!(!state.exists());
        let entries: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries, [std::ffi::OsString::from("example")]);
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
