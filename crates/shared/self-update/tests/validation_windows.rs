#![cfg(windows)]

use std::time::Duration;

use sha2::{Digest, Sha256};
use soma_self_update::{UpdateDirective, UpdateError, UpdateLayout, UpdatePolicy, Updater};
use tempfile::tempdir;

async fn cancellable_validator(
    timeout: Duration,
) -> (
    tempfile::TempDir,
    Updater,
    soma_self_update::StagedArtifact,
    std::path::PathBuf,
) {
    let temp = tempdir().unwrap();
    let source = temp.path().join("cancellable-validator.rs");
    let fixture = temp.path().join("cancellable-validator.exe");
    std::fs::write(
        &source,
        r#"use std::{fs, process::{Command, Stdio}, time::Duration};
fn main() {
    let executable = std::env::current_exe().unwrap();
    let child = Command::new("cmd.exe")
        .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    fs::write(executable.with_extension("part.child"), child.id().to_string()).unwrap();
    std::thread::sleep(Duration::from_secs(30));
}
"#,
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    assert!(
        std::process::Command::new(rustc)
            .arg(&source)
            .arg("-o")
            .arg(&fixture)
            .status()
            .unwrap()
            .success()
    );
    let bytes = std::fs::read(&fixture).unwrap();
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let updater = Updater::new(
        UpdateLayout::new(
            temp.path().join("agent.exe"),
            temp.path().join("state.json"),
        ),
        UpdatePolicy::default()
            .with_validation_timeout(timeout)
            .unwrap(),
    );
    let directive = UpdateDirective::new("2.0.0", "/agent", digest).unwrap();
    let staged = updater.stage(&bytes[..], &directive).await.unwrap();
    let child_file = staged.path().with_extension("part.child");
    (temp, updater, staged, child_file)
}

#[tokio::test]
async fn timeout_kills_a_windows_validator_process_tree() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("validator.rs");
    let fixture = temp.path().join("validator.exe");
    std::fs::write(
        &source,
        r#"use std::{fs, process::{Command, Stdio}, time::Duration};
fn main() {
    let executable = std::env::current_exe().unwrap();
    let child = Command::new("cmd.exe")
        .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    fs::write(executable.with_extension("part.child"), child.id().to_string()).unwrap();
    println!("validator 2.0.0");
    std::thread::sleep(Duration::from_secs(30));
}

"#,
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .arg(&source)
        .arg("-o")
        .arg(&fixture)
        .status()
        .unwrap();
    assert!(status.success());
    let bytes = std::fs::read(&fixture).unwrap();
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let updater = Updater::new(
        UpdateLayout::new(
            temp.path().join("agent.exe"),
            temp.path().join("state.json"),
        ),
        UpdatePolicy::default()
            .with_validation_timeout(Duration::from_millis(250))
            .unwrap(),
    );
    let directive = UpdateDirective::new("2.0.0", "/agent", digest).unwrap();
    let staged = updater.stage(&bytes[..], &directive).await.unwrap();
    let child_file = staged.path().with_extension("part.child");

    assert!(matches!(
        updater.validate(staged).await,
        Err(UpdateError::ValidationTimedOut { .. })
    ));
    let pid = std::fs::read_to_string(child_file).unwrap();
    let mut attempts = 0;
    let last_listing = loop {
        let output = std::process::Command::new("tasklist.exe")
            .args([
                "/FI",
                &format!("PID eq {}", pid.trim()),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout).into_owned();
        if !listing.contains(pid.trim()) {
            return;
        }
        attempts += 1;
        if attempts == 100 {
            break listing;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    panic!("validator descendant {pid} survived job termination: {last_listing}");
}

#[tokio::test]
async fn aborting_validation_kills_a_windows_validator_job() {
    let (_temp, updater, staged, child_file) = cancellable_validator(Duration::from_secs(30)).await;
    let validation = tokio::spawn(async move { updater.validate(staged).await });
    for _ in 0..200 {
        if child_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let pid = std::fs::read_to_string(child_file).unwrap();
    validation.abort();
    let _ = validation.await;

    for _ in 0..100 {
        let output = std::process::Command::new("tasklist.exe")
            .args([
                "/FI",
                &format!("PID eq {}", pid.trim()),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            .unwrap();
        if !String::from_utf8_lossy(&output.stdout).contains(pid.trim()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "validator descendant {} survived job cancellation",
        pid.trim()
    );
}

#[tokio::test]
async fn successful_validation_terminates_a_pipe_inheriting_windows_helper() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("successful-validator.rs");
    let fixture = temp.path().join("successful-validator.exe");
    std::fs::write(
        &source,
        r#"use std::{fs, process::{Command, Stdio}};
fn main() {
    let executable = std::env::current_exe().unwrap();
    let child = Command::new("cmd.exe")
        .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    fs::write(executable.with_extension("part.child"), child.id().to_string()).unwrap();
    println!("validator 2.0.0");
}
"#,
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    assert!(
        std::process::Command::new(rustc)
            .arg(&source)
            .arg("-o")
            .arg(&fixture)
            .status()
            .unwrap()
            .success()
    );
    let bytes = std::fs::read(&fixture).unwrap();
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let updater = Updater::new(
        UpdateLayout::new(
            temp.path().join("agent.exe"),
            temp.path().join("state.json"),
        ),
        UpdatePolicy::default()
            .with_validation_timeout(Duration::from_secs(5))
            .unwrap(),
    );
    let directive = UpdateDirective::new("2.0.0", "/agent", digest).unwrap();
    let staged = updater.stage(&bytes[..], &directive).await.unwrap();
    let child_file = staged.path().with_extension("part.child");

    let result = updater.validate(staged).await;
    assert!(result.is_ok(), "{result:?}");
    let pid = std::fs::read_to_string(child_file).unwrap();
    for _ in 0..100 {
        let output = std::process::Command::new("tasklist.exe")
            .args([
                "/FI",
                &format!("PID eq {}", pid.trim()),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            .unwrap();
        if !String::from_utf8_lossy(&output.stdout).contains(pid.trim()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "validator helper {} survived successful validation",
        pid.trim()
    );
}
