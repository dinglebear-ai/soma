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
