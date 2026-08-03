use std::future::Future;
use std::time::Duration;

use async_trait::async_trait;
use bollard::models::{ContainerCreateBody, ContainerInspectResponse, NetworkingConfig};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, StartContainerOptions, StopContainerOptions,
};
use futures_util::StreamExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use soma_fleet::{HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::docker_map::map_container_inspect;
use crate::{
    BollardReadClient, ContainerRecreateFingerprint, ContainerRecreateInspector,
    ContainerRecreateMutator, ContainerRecreateReceipt, ContainerRecreateRequest,
    ContainerRecreateStage, InfraError, InfraResult, MutationFailure, MutationResult,
};

#[async_trait]
impl ContainerRecreateInspector for BollardReadClient {
    async fn recreate_fingerprint(
        &self,
        host: &HostRecord,
        container: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerRecreateFingerprint> {
        self.validate_host(host)?;
        let raw = tokio::select! {
            () = cancellation.cancelled() => return Err(soma_fleet::FleetError::Cancelled.into()),
            result = self.docker().inspect_container(container, None) => result
                .map_err(|error| InfraError::Docker(error.to_string()))?,
        };
        fingerprint(host, container, &raw)
    }
}

#[async_trait]
impl ContainerRecreateMutator for BollardReadClient {
    async fn recreate_container(
        &self,
        host: &HostRecord,
        request: &ContainerRecreateRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerRecreateReceipt> {
        self.validate_host(host)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        ensure_not_expired(request.deadline(), cancellation)?;
        let raw = self
            .docker()
            .inspect_container(&request.expected().container, None)
            .await
            .map_err(|error| {
                MutationFailure::new(
                    MutationSendState::NotSent,
                    InfraError::Docker(error.to_string()),
                )
            })?;
        let current = fingerprint(host, &request.expected().container, &raw)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        if current != *request.expected() {
            return Err(MutationFailure::new(
                MutationSendState::NotSent,
                InfraError::InvalidRequest {
                    domain: "container-recreate",
                    message: "container configuration changed immediately before replacement"
                        .into(),
                },
            ));
        }

        let image = image_ref(&raw)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let name = container_name(&raw, &request.expected().container);
        if request.pull() {
            pull_image(self, &image, request.deadline(), cancellation).await?;
        }

        let mut stage = ContainerRecreateStage::Prepared;
        await_stage(
            request.deadline(),
            cancellation,
            stage,
            self.docker()
                .stop_container(&request.expected().container, None::<StopContainerOptions>),
        )
        .await?;
        stage = ContainerRecreateStage::Stopped;

        await_stage(
            request.deadline(),
            cancellation,
            stage,
            self.docker()
                .remove_container(&request.expected().container, None),
        )
        .await?;
        stage = ContainerRecreateStage::Removed;

        let body = create_body(&raw, &image);
        let created = await_stage(
            request.deadline(),
            cancellation,
            stage,
            self.docker().create_container(
                Some(CreateContainerOptions {
                    name: Some(name.clone()),
                    platform: String::new(),
                }),
                body,
            ),
        )
        .await?;
        stage = ContainerRecreateStage::Created;

        await_stage(
            request.deadline(),
            cancellation,
            stage,
            self.docker()
                .start_container(&created.id, None::<StartContainerOptions>),
        )
        .await?;
        stage = ContainerRecreateStage::Started;

        Ok(ContainerRecreateReceipt {
            host: host.id().clone(),
            topology_revision: TopologyRevision::clone(host.revision()),
            original_container: request.expected().container.clone(),
            new_container: Some(created.id),
            name,
            image,
            stage,
            send_state: MutationSendState::Sent,
            pulled: request.pull(),
        })
    }
}

fn fingerprint(
    host: &HostRecord,
    container: &str,
    raw: &ContainerInspectResponse,
) -> InfraResult<ContainerRecreateFingerprint> {
    let value = serde_json::to_value(raw).map_err(|error| InfraError::Parse {
        domain: "container-recreate",
        message: error.to_string(),
    })?;
    let neutral = map_container_inspect(host, &value)?;
    let name = container_name(raw, container);
    let image = image_ref(raw)?;
    let material = json!({
        "name": name,
        "image": image,
        "config": raw.config,
        "host_config": raw.host_config,
        "networks": raw.network_settings.as_ref().and_then(|settings| settings.networks.as_ref()),
    });
    let encoded = serde_json::to_vec(&material).map_err(|error| InfraError::Parse {
        domain: "container-recreate",
        message: error.to_string(),
    })?;
    let sha256 = format!("{:x}", Sha256::digest(encoded));
    ContainerRecreateFingerprint::new(container, name, image, neutral.state, sha256)
}

fn image_ref(raw: &ContainerInspectResponse) -> InfraResult<String> {
    raw.config
        .as_ref()
        .and_then(|config| config.image.clone())
        .filter(|image| !image.is_empty())
        .ok_or_else(|| InfraError::Parse {
            domain: "container-recreate",
            message: "container inspection does not include an image reference".into(),
        })
}

fn container_name(raw: &ContainerInspectResponse, fallback: &str) -> String {
    raw.name
        .as_deref()
        .map(|name| name.trim_start_matches('/').to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

fn create_body(raw: &ContainerInspectResponse, image: &str) -> ContainerCreateBody {
    let config = raw.config.as_ref();
    let networking_config = raw
        .network_settings
        .as_ref()
        .and_then(|settings| settings.networks.as_ref())
        .map(|networks| NetworkingConfig {
            endpoints_config: Some(networks.clone()),
        });
    ContainerCreateBody {
        image: Some(image.to_owned()),
        env: config.and_then(|config| config.env.clone()),
        cmd: config.and_then(|config| config.cmd.clone()),
        entrypoint: config.and_then(|config| config.entrypoint.clone()),
        labels: config.and_then(|config| config.labels.clone()),
        working_dir: config.and_then(|config| config.working_dir.clone()),
        user: config.and_then(|config| config.user.clone()),
        volumes: config.and_then(|config| config.volumes.clone()),
        host_config: raw.host_config.clone(),
        networking_config,
        ..Default::default()
    }
}

async fn pull_image(
    client: &BollardReadClient,
    image: &str,
    deadline: Timestamp,
    cancellation: &CancellationToken,
) -> MutationResult<()> {
    let (from_image, tag) = split_image(image);
    let mut stream = client.docker().create_image(
        Some(CreateImageOptions {
            from_image: Some(from_image),
            tag,
            ..Default::default()
        }),
        None,
        None,
    );
    loop {
        let remaining = remaining(deadline)?;
        let item = tokio::select! {
            () = cancellation.cancelled() => return Err(MutationFailure::new(
                MutationSendState::Unknown,
                soma_fleet::FleetError::Cancelled.into(),
            )),
            result = tokio::time::timeout(remaining, stream.next()) => match result {
                Err(_) => return Err(MutationFailure::new(
                    MutationSendState::Unknown,
                    soma_fleet::FleetError::DeadlineExceeded.into(),
                )),
                Ok(value) => value,
            }
        };
        match item {
            None => return Ok(()),
            Some(Ok(frame)) => {
                let value = serde_json::to_value(frame).map_err(|error| {
                    MutationFailure::new(
                        MutationSendState::Sent,
                        InfraError::Parse {
                            domain: "container-recreate",
                            message: error.to_string(),
                        },
                    )
                })?;
                let error = value
                    .get("error_detail")
                    .or_else(|| value.get("errorDetail"))
                    .and_then(|detail| detail.get("message"))
                    .and_then(serde_json::Value::as_str);
                if let Some(error) = error {
                    return Err(MutationFailure::new(
                        MutationSendState::Sent,
                        InfraError::Docker(error.to_owned()),
                    ));
                }
            }
            Some(Err(error)) => {
                return Err(MutationFailure::new(
                    MutationSendState::Unknown,
                    InfraError::Docker(error.to_string()),
                ));
            }
        }
    }
}

fn split_image(image: &str) -> (String, Option<String>) {
    if image.contains('@') {
        return (image.to_owned(), None);
    }
    let slash = image.rfind('/');
    let colon = image.rfind(':');
    match colon.filter(|colon| slash.is_none_or(|slash| *colon > slash)) {
        Some(colon) => (
            image[..colon].to_owned(),
            Some(image[colon + 1..].to_owned()),
        ),
        None => (image.to_owned(), Some("latest".into())),
    }
}

fn ensure_not_expired(deadline: Timestamp, cancellation: &CancellationToken) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    remaining(deadline).map(|_| ())
}

fn remaining(deadline: Timestamp) -> MutationResult<Duration> {
    let millis = deadline
        .unix_millis()
        .saturating_sub(Timestamp::now().unix_millis());
    if millis <= 0 {
        Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ))
    } else {
        Ok(Duration::from_millis(millis as u64))
    }
}

async fn await_stage<T, F>(
    deadline: Timestamp,
    cancellation: &CancellationToken,
    stage: ContainerRecreateStage,
    future: F,
) -> MutationResult<T>
where
    F: Future<Output = Result<T, bollard::errors::Error>>,
{
    let timeout = remaining(deadline)?;
    tokio::select! {
        () = cancellation.cancelled() => Err(MutationFailure::new(
            MutationSendState::Unknown,
            InfraError::Docker(format!("container recreate cancelled after stage {stage:?}")),
        )),
        result = tokio::time::timeout(timeout, future) => match result {
            Err(_) => Err(MutationFailure::new(
                MutationSendState::Unknown,
                InfraError::Docker(format!("container recreate timed out after stage {stage:?}")),
            )),
            Ok(Err(error)) => Err(MutationFailure::new(
                MutationSendState::Unknown,
                InfraError::Docker(format!("container recreate failed after stage {stage:?}: {error}")),
            )),
            Ok(Ok(value)) => Ok(value),
        }
    }
}

#[cfg(test)]
#[path = "bollard_recreate_tests.rs"]
mod tests;
