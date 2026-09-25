use std::path::Path;

use soma_fleet::HostRecord;
use soma_infra::{FileTransferFingerprint, ImageRemovalFingerprint};
use soma_ops::{OperationContext, OperationName, PlannedChange, TargetKind, TargetRef, Timestamp};

use crate::mutation_compose::compose_target;
use crate::mutation_runtime::DEFAULT_MUTATION_DEADLINE_MS;
use crate::{ExecutionError, SynapseFinalPorts, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    pub(crate) fn final_ports(
        &self,
        domain: &'static str,
    ) -> Result<&SynapseFinalPorts, ExecutionError> {
        self.ports
            .final_mutations
            .as_ref()
            .ok_or_else(|| ExecutionError::MutationPortUnavailable {
                domain,
                host: "unresolved".into(),
            })
    }
}

pub(crate) fn final_operation(operation: &OperationName) -> bool {
    matches!(
        operation.as_str(),
        "docker.rmi" | "docker.prune" | "compose.down" | "files.transfer"
    )
}

pub(crate) fn image_target(host: &HostRecord, image: &str) -> Result<TargetRef, ExecutionError> {
    TargetRef::new(TargetKind::Image, image)?
        .with_host(host.id().to_string())?
        .with_revision(host.revision().to_string())
        .map_err(ExecutionError::from)
}

pub(crate) fn docker_target(host: &HostRecord) -> Result<TargetRef, ExecutionError> {
    TargetRef::new(TargetKind::DockerDaemon, host.id().to_string())?
        .with_host(host.id().to_string())?
        .with_revision(host.revision().to_string())
        .map_err(ExecutionError::from)
}

pub(crate) fn transfer_target(
    source: &HostRecord,
    source_path: &Path,
    destination: &HostRecord,
    destination_path: &Path,
) -> Result<TargetRef, ExecutionError> {
    let parent = TargetRef::new(TargetKind::File, source_path.to_string_lossy())?
        .with_host(source.id().to_string())?
        .with_revision(source.revision().to_string())?;
    TargetRef::new(TargetKind::File, destination_path.to_string_lossy())?
        .with_host(destination.id().to_string())?
        .with_parent(parent)?
        .with_revision(destination.revision().to_string())
        .map_err(ExecutionError::from)
}

pub(crate) fn rmi_change(
    host: &HostRecord,
    fingerprint: &ImageRemovalFingerprint,
    force: bool,
) -> Result<PlannedChange, ExecutionError> {
    Ok(PlannedChange::new(
        image_target(host, &fingerprint.reference)?,
        if force { "remove_force" } else { "remove" },
        format!(
            "remove image {} resolved from {}",
            fingerprint.identity.id, fingerprint.reference
        ),
    )?
    .with_digests(Some(fingerprint.sha256.clone()), None))
}

pub(crate) fn prune_change(
    host: &HostRecord,
    fingerprint: &soma_infra::DockerPruneFingerprint,
    force: bool,
) -> Result<PlannedChange, ExecutionError> {
    Ok(PlannedChange::new(
        docker_target(host)?,
        if force { "prune_force" } else { "prune" },
        format!(
            "prune Docker {} candidate inventory",
            fingerprint.target.as_str()
        ),
    )?
    .with_digests(Some(fingerprint.sha256.clone()), None))
}

pub(crate) fn compose_down_change(
    host: &HostRecord,
    fingerprint: &soma_infra::ComposeRecreateFingerprint,
    force: bool,
    remove_volumes: bool,
) -> Result<PlannedChange, ExecutionError> {
    let action = match (force, remove_volumes) {
        (_, true) => "down_remove_volumes",
        (true, false) => "down_force",
        (false, false) => "down",
    };
    Ok(PlannedChange::new(
        compose_target(host, &fingerprint.project)?,
        action,
        format!(
            "tear down {} services in Compose project {}",
            fingerprint.services.len(),
            fingerprint.project
        ),
    )?
    .with_digests(Some(fingerprint.sha256.clone()), None))
}

pub(crate) fn transfer_change(
    target: &TargetRef,
    fingerprint: &FileTransferFingerprint,
) -> Result<PlannedChange, ExecutionError> {
    Ok(PlannedChange::new(
        target.clone(),
        "copy_verified",
        format!(
            "copy {} verified bytes to {}",
            fingerprint.source.bytes,
            fingerprint.destination_path.display()
        ),
    )?
    .with_digests(
        fingerprint
            .destination_before
            .as_ref()
            .map(|identity| identity.sha256.clone()),
        Some(fingerprint.source.sha256.clone()),
    ))
}

pub(crate) fn planning_deadline(context: &OperationContext) -> Timestamp {
    context.deadline().unwrap_or_else(|| {
        Timestamp::from_unix_millis(
            Timestamp::now()
                .unix_millis()
                .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
        )
    })
}

#[cfg(test)]
#[path = "mutation_final_contract_tests.rs"]
mod tests;
