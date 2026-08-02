use std::collections::BTreeMap;

use serde_json::Value;
use soma_fleet::HostRecord;

use crate::{
    ContainerInspect, ContainerState, ContainerSummary, DockerSystemInfo, ImageSummary, InfraError,
    InfraResult, NetworkSummary, VolumeSummary,
};

pub(crate) fn map_system_info(host: &HostRecord, value: &Value) -> InfraResult<DockerSystemInfo> {
    Ok(DockerSystemInfo {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        daemon_id: string(value, &["ID", "Id", "id"]),
        name: string(value, &["Name", "name"]),
        server_version: string(value, &["ServerVersion", "server_version"]),
        operating_system: string(value, &["OperatingSystem", "operating_system"]),
        architecture: string(value, &["Architecture", "architecture"]),
        kernel_version: string(value, &["KernelVersion", "kernel_version"]),
        storage_driver: string(value, &["Driver", "driver"]),
        containers: unsigned(value, &["Containers", "containers"]),
        containers_running: unsigned(value, &["ContainersRunning", "containers_running"]),
        containers_paused: unsigned(value, &["ContainersPaused", "containers_paused"]),
        containers_stopped: unsigned(value, &["ContainersStopped", "containers_stopped"]),
        images: unsigned(value, &["Images", "images"]),
        cpus: unsigned(value, &["NCPU", "ncpu"]),
        memory_total_bytes: unsigned(value, &["MemTotal", "mem_total"]),
    })
}

pub(crate) fn map_container_summary(
    host: &HostRecord,
    value: &Value,
) -> InfraResult<ContainerSummary> {
    Ok(ContainerSummary {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        id: string(value, &["Id", "ID", "id"]),
        names: strings(value, &["Names", "names"]),
        image: string(value, &["Image", "image"]),
        image_id: string(value, &["ImageID", "image_id"]),
        command: string(value, &["Command", "command"]),
        created_unix_seconds: signed_optional(value, &["Created", "created"]),
        state: ContainerState::from_text(string(value, &["State", "state"]).as_deref()),
        status: string(value, &["Status", "status"]),
        labels: string_map(value, &["Labels", "labels"]),
    })
}

pub(crate) fn map_container_inspect(
    host: &HostRecord,
    value: &Value,
) -> InfraResult<ContainerInspect> {
    let state = object_field(value, &["State", "state"]);
    let state_text = state.and_then(|value| string(value, &["Status", "status"]));
    let config = object_field(value, &["Config", "config"]);
    Ok(ContainerInspect {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        id: string(value, &["Id", "ID", "id"]),
        name: string(value, &["Name", "name"]),
        created: string(value, &["Created", "created"]),
        path: string(value, &["Path", "path"]),
        args: strings(value, &["Args", "args"]),
        image: string(value, &["Image", "image"]),
        state: ContainerState::from_text(state_text.as_deref()),
        pid: state.and_then(|value| signed_optional(value, &["Pid", "pid"])),
        exit_code: state.and_then(|value| signed_optional(value, &["ExitCode", "exit_code"])),
        restart_count: signed_optional(value, &["RestartCount", "restart_count"]),
        labels: config.map_or_else(BTreeMap::new, |value| {
            string_map(value, &["Labels", "labels"])
        }),
    })
}

pub(crate) fn map_image(host: &HostRecord, value: &Value) -> InfraResult<ImageSummary> {
    let id = string(value, &["Id", "ID", "id"]).ok_or_else(|| parse_error("image has no ID"))?;
    Ok(ImageSummary {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        id,
        repo_tags: strings(value, &["RepoTags", "repo_tags"]),
        repo_digests: strings(value, &["RepoDigests", "repo_digests"]),
        created_unix_seconds: signed(value, &["Created", "created"]),
        size_bytes: signed(value, &["Size", "size"]),
        containers: signed(value, &["Containers", "containers"]),
        labels: string_map(value, &["Labels", "labels"]),
    })
}

pub(crate) fn map_network(host: &HostRecord, value: &Value) -> InfraResult<NetworkSummary> {
    Ok(NetworkSummary {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        id: string(value, &["Id", "ID", "id"]),
        name: string(value, &["Name", "name"]),
        driver: string(value, &["Driver", "driver"]),
        scope: string(value, &["Scope", "scope"]),
        internal: boolean(value, &["Internal", "internal"]),
        attachable: boolean(value, &["Attachable", "attachable"]),
        labels: string_map(value, &["Labels", "labels"]),
    })
}

pub(crate) fn map_volume(host: &HostRecord, value: &Value) -> InfraResult<VolumeSummary> {
    Ok(VolumeSummary {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        name: string(value, &["Name", "name"]).ok_or_else(|| parse_error("volume has no name"))?,
        driver: string(value, &["Driver", "driver"]).unwrap_or_default(),
        mountpoint: string(value, &["Mountpoint", "mountpoint"]).unwrap_or_default(),
        scope: string(value, &["Scope", "scope"]),
        labels: string_map(value, &["Labels", "labels"]),
    })
}

pub(crate) fn parse_error(message: impl Into<String>) -> InfraError {
    InfraError::Parse {
        domain: "docker",
        message: message.into(),
    }
}

fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    names.iter().find_map(|name| object.get(*name))
}

fn object_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    field(value, names).filter(|value| value.is_object())
}

pub(crate) fn array_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Vec<Value>> {
    field(value, names)?.as_array()
}

fn string(value: &Value, names: &[&str]) -> Option<String> {
    field(value, names)?.as_str().map(str::to_owned)
}

fn strings(value: &Value, names: &[&str]) -> Vec<String> {
    array_field(value, names)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn string_map(value: &Value, names: &[&str]) -> BTreeMap<String, String> {
    field(value, names)
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

fn unsigned(value: &Value, names: &[&str]) -> u64 {
    field(value, names)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
        })
        .unwrap_or_default()
}

fn signed(value: &Value, names: &[&str]) -> i64 {
    signed_optional(value, names).unwrap_or_default()
}

fn signed_optional(value: &Value, names: &[&str]) -> Option<i64> {
    field(value, names)?.as_i64().or_else(|| {
        field(value, names)?
            .as_u64()
            .and_then(|value| i64::try_from(value).ok())
    })
}

fn boolean(value: &Value, names: &[&str]) -> Option<bool> {
    field(value, names)?.as_bool()
}

#[cfg(test)]
#[path = "docker_map_tests.rs"]
mod tests;
