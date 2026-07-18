//! Compile-checked lifecycle sketch for a heartbeat-driven adopter.

use std::ffi::OsString;

use serde::Deserialize;
use soma_self_update::{
    InstallOutcome, RecoveryAction, UpdateDirective, UpdateLayout, UpdatePolicy, Updater,
};
use url::Url;

#[derive(Deserialize)]
struct CallerDirective {
    version: String,
    artifact_url: String,
    sha256: String,
}

fn authenticate_directive(json: &str) -> soma_self_update::Result<UpdateDirective> {
    // Real caller code verifies its authenticated heartbeat response or a
    // detached signature before constructing the library directive.
    let value: CallerDirective = serde_json::from_str(json)
        .map_err(|_| soma_self_update::UpdateError::InvalidDirective("invalid caller JSON"))?;
    UpdateDirective::new(value.version, value.artifact_url, value.sha256)
}

async fn fetch_artifact(_url: &Url) -> impl tokio::io::AsyncRead + Unpin {
    // A real adopter returns its reqwest response stream adapter here.
    tokio::io::empty()
}

async fn lifecycle() -> soma_self_update::Result<()> {
    let updater = Updater::new(
        UpdateLayout::new("/opt/example/bin/example", "/opt/example/state/update.json"),
        UpdatePolicy::default(),
    );

    match updater.recover_on_startup("1.0.0").await? {
        RecoveryAction::RollbackInstalled { .. } => return Ok(()), // ask supervisor to restart
        RecoveryAction::NoPendingUpdate
        | RecoveryAction::PendingUpdate { .. }
        | RecoveryAction::StaleMarkerRemoved { .. } => {}
    }

    let json = r#"{"version":"2.0.0","artifact_url":"/v1/agent/binary","sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#;
    let directive = authenticate_directive(json)?;
    let endpoint = Url::parse("https://host/v1/heartbeats").expect("static endpoint is valid");
    let artifact_url = directive.resolve_artifact_url(&endpoint, updater.policy().transport())?;
    let reader = fetch_artifact(&artifact_url).await;
    let staged = updater.stage(reader, &directive).await?;
    let validated = updater.validate(staged).await?;
    let InstallOutcome::RestartRequired { executable, .. } =
        updater.install(validated, "1.0.0").await?;

    #[cfg(unix)]
    {
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        let never = soma_self_update::reexec(&executable, args)?;
        match never {}
    }
    #[cfg(not(unix))]
    return Err(soma_self_update::UpdateError::UnsupportedPlatform);
}

async fn first_successful_health_report(
    updater: &Updater,
    version: &str,
) -> soma_self_update::Result<()> {
    send_authenticated_health_report().await;
    updater.confirm_success(version).await?;
    Ok(())
}

async fn send_authenticated_health_report() {}

fn main() {
    // The example is compile-checked but deliberately does not replace a live
    // binary. An adopter calls `lifecycle`, enters its service loop, then calls
    // `first_successful_health_report` only after its first healthy report.
    let _ = lifecycle;
    let _ = first_successful_health_report;
}
