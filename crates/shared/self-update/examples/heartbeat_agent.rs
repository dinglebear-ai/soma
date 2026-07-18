//! Compile-checked lifecycle sketch for a heartbeat-driven adopter.

#[cfg(unix)]
use std::ffi::OsString;

use serde::Deserialize;
#[cfg(unix)]
use soma_self_update::InstallOutcome;
use soma_self_update::{RecoveryAction, UpdateDirective, UpdateLayout, UpdatePolicy, Updater};
use url::Url;

#[derive(Deserialize)]
struct CallerDirective {
    version: String,
    artifact_url: String,
    sha256: String,
}

struct VerifiedHeartbeatPayload(Vec<u8>);

fn verify_heartbeat_payload(response: &[u8]) -> soma_self_update::Result<VerifiedHeartbeatPayload> {
    // Real caller code verifies the heartbeat MAC, authenticated transport
    // identity, or detached publisher signature here. Only verified bytes may
    // cross this boundary into directive parsing.
    let heartbeat_signature_is_valid = true;
    if !heartbeat_signature_is_valid {
        return Err(soma_self_update::UpdateError::InvalidDirective(
            "heartbeat authentication failed",
        ));
    }
    Ok(VerifiedHeartbeatPayload(response.to_vec()))
}

fn directive_from_verified_payload(
    payload: &VerifiedHeartbeatPayload,
) -> soma_self_update::Result<UpdateDirective> {
    let value: CallerDirective = serde_json::from_slice(&payload.0)
        .map_err(|_| soma_self_update::UpdateError::InvalidDirective("invalid caller JSON"))?;
    UpdateDirective::new(value.version, value.artifact_url, value.sha256)
}

async fn fetch_artifact(url: &Url) -> (Url, impl tokio::io::AsyncRead + Unpin) {
    // A real adopter disables automatic redirects, validates every redirect
    // target with `validate_artifact_response_url`, and returns the final URL
    // together with its response stream.
    (url.clone(), tokio::io::empty())
}

async fn lifecycle() -> soma_self_update::Result<()> {
    let updater = Updater::new(
        UpdateLayout::new("/opt/example/bin/example", "/opt/example/state/update.json"),
        UpdatePolicy::default(),
    );

    match updater.recover_on_startup("1.0.0").await? {
        RecoveryAction::RollbackInstalled { .. } => return Ok(()), // ask supervisor to restart
        RecoveryAction::NoPendingUpdate | RecoveryAction::PendingUpdate { .. } => {}
    }

    let response = br#"{"version":"2.0.0","artifact_url":"/v1/agent/binary","sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#;
    let verified_payload = verify_heartbeat_payload(response)?;
    let directive = directive_from_verified_payload(&verified_payload)?;
    let endpoint = Url::parse("https://host/v1/heartbeats").expect("static endpoint is valid");
    let artifact_url = directive.resolve_artifact_url(&endpoint, updater.policy().transport())?;
    let (response_url, reader) = fetch_artifact(&artifact_url).await;
    directive.validate_artifact_response_url(
        &endpoint,
        &response_url,
        updater.policy().transport(),
    )?;
    let staged = updater.stage(reader, &directive).await?;
    let validated = updater.validate(staged).await?;
    let outcome = updater.install(validated, "1.0.0").await?;

    #[cfg(unix)]
    {
        let InstallOutcome::RestartRequired { executable, .. } = outcome;
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        let never = soma_self_update::reexec(&executable, args)?;
        match never {}
    }
    #[cfg(not(unix))]
    {
        let _ = outcome;
        Err(soma_self_update::UpdateError::UnsupportedPlatform)
    }
}

async fn first_successful_health_report(
    updater: &Updater,
    version: &str,
) -> soma_self_update::Result<()> {
    send_authenticated_health_report().await?;
    updater.confirm_success(version).await?;
    Ok(())
}

async fn send_authenticated_health_report() -> soma_self_update::Result<()> {
    Ok(())
}

fn main() {
    // The example is compile-checked but deliberately does not replace a live
    // binary. An adopter calls `lifecycle`, enters its service loop, then calls
    // `first_successful_health_report` only after its first healthy report.
    let _ = lifecycle;
    let _ = first_successful_health_report;
}
