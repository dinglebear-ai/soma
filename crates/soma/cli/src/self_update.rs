//! `soma self-update` — operator-driven binary self-update (CLI infrastructure).
//!
//! Thin adapter over the shared `soma-self-update` transaction crate:
//! `run` downloads, stages, validates, and installs a new binary over the
//! running executable; `recover` reconciles pending transaction state after a
//! restart; `confirm` finalizes an update once the restarted service is
//! healthy. Like `doctor` and `watch`, this is process infrastructure, not a
//! service action — it has no MCP or REST parity requirement.
//!
//! The operator supplies the directive (version, artifact URL, SHA-256) and is
//! responsible for authenticating it — for example against a release page or a
//! signed manifest — before typing it here. A digest fetched from the same
//! server as the artifact proves transit integrity, not publisher identity.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use soma_self_update::{
    ArtifactTransportPolicy, ConfirmationOutcome, InstallOutcome, RecoveryAction, UpdateDirective,
    UpdateLayout, UpdatePolicy, Updater,
};
use url::Url;

/// Parsed `soma self-update` subcommand.
#[derive(Debug, PartialEq, Eq)]
pub enum SelfUpdateCommand {
    /// Download, stage, validate, and install a new binary.
    Run {
        version: String,
        url: String,
        sha256: String,
        allow_http_loopback: bool,
        state_file: Option<String>,
    },
    /// Reconcile pending update state; call before entering normal service.
    Recover { state_file: Option<String> },
    /// Confirm a pending update after the restarted binary is healthy.
    Confirm { state_file: Option<String> },
}

/// Dispatch a parsed self-update subcommand against the running executable.
pub async fn run_self_update(command: SelfUpdateCommand, running_version: &str) -> Result<()> {
    match command {
        SelfUpdateCommand::Run {
            version,
            url,
            sha256,
            allow_http_loopback,
            state_file,
        } => {
            let transport = transport_policy(allow_http_loopback);
            let updater = build_updater(state_file, transport)?;
            run_update(&updater, version, url, sha256, transport, running_version).await
        }
        SelfUpdateCommand::Recover { state_file } => {
            let updater = build_updater(state_file, ArtifactTransportPolicy::HttpsOnly)?;
            match updater.recover_on_startup(running_version).await? {
                RecoveryAction::NoPendingUpdate => println!("no pending update"),
                RecoveryAction::PendingUpdate {
                    target,
                    attempts,
                    max_attempts,
                } => println!(
                    "pending update to {target} (unconfirmed startup {attempts}/{max_attempts}); \
                     run `soma self-update confirm` once the service is healthy"
                ),
                RecoveryAction::RollbackInstalled {
                    executable,
                    restored_version,
                } => println!(
                    "rolled back to {restored_version}; restart {}",
                    executable.display()
                ),
            }
            Ok(())
        }
        SelfUpdateCommand::Confirm { state_file } => {
            let updater = build_updater(state_file, ArtifactTransportPolicy::HttpsOnly)?;
            // A failed confirmation repeats on every attempt (for example when
            // the rollback backup is missing) and needs operator attention —
            // surface it as a hard error, never retry silently.
            match updater.confirm_success(running_version).await? {
                ConfirmationOutcome::NoPendingUpdate => println!("no pending update to confirm"),
                ConfirmationOutcome::Confirmed { version } => {
                    println!("update to {version} confirmed; rollback backup removed");
                }
            }
            Ok(())
        }
    }
}

fn transport_policy(allow_http_loopback: bool) -> ArtifactTransportPolicy {
    if allow_http_loopback {
        ArtifactTransportPolicy::HttpsOrLoopbackHttp
    } else {
        ArtifactTransportPolicy::HttpsOnly
    }
}

fn build_updater(
    state_file: Option<String>,
    transport: ArtifactTransportPolicy,
) -> Result<Updater> {
    let executable =
        std::env::current_exe().context("cannot resolve the running executable path")?;
    // The transaction crate rejects symlinked executable leaves; canonicalize
    // so an installation reached through a symlinked path still targets the
    // real file.
    let executable = std::fs::canonicalize(&executable)
        .with_context(|| format!("cannot canonicalize {}", executable.display()))?;
    let state_file = match state_file {
        Some(path) => PathBuf::from(path),
        None => default_state_file(&executable)?,
    };
    let policy = UpdatePolicy::default().with_transport(transport);
    Ok(Updater::new(
        UpdateLayout::new(executable, state_file),
        policy,
    ))
}

/// Default durable transaction marker path: a hidden sibling of the
/// executable, so update state shares the directory (and durability domain)
/// the installer already requires to be trusted and writable.
fn default_state_file(executable: &Path) -> Result<PathBuf> {
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("executable name must be valid UTF-8"))?;
    Ok(executable.with_file_name(format!(".{name}.update-state.json")))
}

async fn run_update(
    updater: &Updater,
    version: String,
    url: String,
    sha256: String,
    transport: ArtifactTransportPolicy,
    running_version: &str,
) -> Result<()> {
    let directive = UpdateDirective::new(version, url, sha256)?;
    // The operator supplies one absolute artifact URL, so it serves as its own
    // same-origin endpoint for resolution and redirect validation.
    let endpoint = Url::parse(directive.artifact_url())
        .map_err(|error| anyhow!("--url must be an absolute URL: {error}"))?;
    let artifact = directive.resolve_artifact_url(&endpoint, transport)?;
    updater.preflight_stage()?;
    let body = download(
        &directive,
        &endpoint,
        &artifact,
        transport,
        updater.policy(),
    )
    .await?;
    let staged = updater.stage(&body[..], &directive).await?;
    let validated = updater.validate(staged).await?;
    match updater.install(validated, running_version).await? {
        InstallOutcome::RestartRequired {
            executable,
            from,
            to,
        } => {
            println!(
                "installed {to} over {from}; restart {} and run `soma self-update confirm` \
                 once the service is healthy",
                executable.display()
            );
        }
        InstallOutcome::RestartRequiredIndeterminate {
            executable,
            from,
            to,
            error,
        } => {
            eprintln!("warning: install durability is indeterminate: {error}");
            println!(
                "installed {to} over {from}; restart {} — startup recovery will reconcile \
                 the pending marker",
                executable.display()
            );
        }
    }
    Ok(())
}

/// Download the artifact with redirects disabled, validating the final
/// response URL and enforcing the policy size cap while streaming.
async fn download(
    directive: &UpdateDirective,
    endpoint: &Url,
    artifact: &Url,
    transport: ArtifactTransportPolicy,
    policy: &UpdatePolicy,
) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("cannot build HTTP client")?;
    let mut response = client
        .get(artifact.clone())
        .send()
        .await
        .with_context(|| format!("artifact request to {artifact} failed"))?;
    if response.status().is_redirection() {
        bail!(
            "artifact URL redirected (HTTP {}); redirects are refused — pass the final URL to --url",
            response.status()
        );
    }
    if !response.status().is_success() {
        bail!("artifact request returned HTTP {}", response.status());
    }
    directive.validate_artifact_response_url(endpoint, response.url(), transport)?;
    let limit = policy.max_artifact_bytes();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("artifact download from {artifact} failed"))?
    {
        if body.len() as u64 + chunk.len() as u64 > limit {
            bail!("artifact exceeds the {limit} byte policy limit");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
#[path = "self_update_tests.rs"]
mod tests;
