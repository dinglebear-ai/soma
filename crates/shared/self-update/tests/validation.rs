#![cfg(unix)]

use std::time::Duration;

use sha2::{Digest, Sha256};
use soma_self_update::{UpdateDirective, UpdateError, UpdateLayout, UpdatePolicy, Updater};
use tempfile::tempdir;

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn staged(
    script: &[u8],
    version: &str,
    timeout: Duration,
) -> (tempfile::TempDir, Updater, soma_self_update::StagedArtifact) {
    let temp = tempdir().unwrap();
    let updater = Updater::new(
        UpdateLayout::new(temp.path().join("example"), temp.path().join("state.json")),
        UpdatePolicy::default()
            .with_validation_timeout(timeout)
            .unwrap(),
    );
    let directive = UpdateDirective::new(version, "/binary", digest(script)).unwrap();
    let artifact = updater.stage(script, &directive).await.unwrap();
    (temp, updater, artifact)
}

#[tokio::test]
async fn accepts_exact_version_token_and_rejects_substrings() {
    let (_temp, updater, artifact) = staged(
        b"#!/bin/sh\necho 'example (1.2.3),'\n",
        "1.2.3",
        Duration::from_secs(1),
    )
    .await;
    let result = updater.validate(artifact).await;
    assert!(result.is_ok(), "{result:?}");

    let (_temp, updater, artifact) = staged(
        b"#!/bin/sh\necho 'example 11.2.30'\n",
        "1.2.3",
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(
        updater.validate(artifact).await,
        Err(UpdateError::VersionMismatch { .. })
    ));
}

#[tokio::test]
async fn reports_exit_utf8_and_output_failures() {
    let (_temp, updater, artifact) = staged(
        b"#!/bin/sh\necho bad >&2\nexit 7\n",
        "1",
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(
        updater.validate(artifact).await,
        Err(UpdateError::ValidationFailed { .. })
    ));

    let (_temp, updater, artifact) =
        staged(b"#!/bin/sh\nprintf '\\377'\n", "1", Duration::from_secs(1)).await;
    assert!(matches!(
        updater.validate(artifact).await,
        Err(UpdateError::InvalidVersionOutput)
    ));

    let (_temp, updater, artifact) = staged(
        b"#!/bin/sh\nyes x | head -c 20000\n",
        "1",
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(
        updater.validate(artifact).await,
        Err(UpdateError::ValidationOutputTooLarge { .. })
    ));
}

#[tokio::test]
async fn times_out_and_kills_the_validator() {
    let (_temp, updater, artifact) = staged(
        b"#!/bin/sh\nsleep 10\necho 1\n",
        "1",
        Duration::from_millis(100),
    )
    .await;
    let result = tokio::time::timeout(Duration::from_secs(2), updater.validate(artifact))
        .await
        .unwrap();
    assert!(matches!(
        result,
        Err(UpdateError::ValidationTimedOut { .. })
    ));
}

#[tokio::test]
async fn successful_validation_terminates_a_pipe_inheriting_helper() {
    let script = b"#!/bin/sh\nsleep 30 &\necho $! > \"$0.child\"\necho 'example 1'\nexit 0\n";
    let (_temp, updater, artifact) = staged(script, "1", Duration::from_secs(2)).await;
    let child_file = artifact.path().with_extension("part.child");
    let validation = tokio::spawn(async move { updater.validate(artifact).await });
    for _ in 0..100 {
        if child_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let pid: u32 = std::fs::read_to_string(&child_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), validation)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_ok(), "{result:?}");
    for _ in 0..100 {
        if !process_is_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("validator descendant {pid} survived process-group termination");
}

#[tokio::test]
async fn successful_validation_terminates_a_pipe_detached_helper() {
    let script = b"#!/bin/sh\n(sleep 30 </dev/null >/dev/null 2>&1) &\necho $! > \"$0.child\"\necho 'example 1'\nexit 0\n";
    let (_temp, updater, artifact) = staged(script, "1", Duration::from_secs(2)).await;
    let child_file = artifact.path().with_extension("part.child");

    let result = tokio::time::timeout(Duration::from_secs(3), updater.validate(artifact))
        .await
        .expect("successful validation did not complete");
    assert!(result.is_ok(), "{result:?}");
    let pid: u32 = std::fs::read_to_string(&child_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    for _ in 0..100 {
        if !process_is_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("validator helper {pid} survived successful validation");
}

#[tokio::test]
async fn aborting_validation_kills_the_validator_process_group() {
    let script = b"#!/bin/sh\nsleep 30 &\necho $! > \"$0.child\"\nwait\n";
    let (_temp, updater, artifact) = staged(script, "1", Duration::from_secs(30)).await;
    let child_file = artifact.path().with_extension("part.child");
    let validation = tokio::spawn(async move { updater.validate(artifact).await });
    for _ in 0..100 {
        if child_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let pid: u32 = std::fs::read_to_string(&child_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(process_is_alive(pid), "validator child was never alive");

    validation.abort();
    let _ = validation.await;
    for _ in 0..100 {
        if !process_is_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("validator descendant {pid} survived validation cancellation");
}

fn process_is_alive(pid: u32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    match kill(Pid::from_raw(pid.try_into().unwrap()), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(error) => panic!("unexpected process liveness error: {error}"),
    }
}
