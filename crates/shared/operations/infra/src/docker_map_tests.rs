use serde_json::json;
use soma_fleet::{HostEndpoint, HostId, HostRecord};

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("nashost").unwrap(), HostEndpoint::Local)
}

#[test]
fn system_and_container_fields_accept_docker_api_names() {
    let system = map_system_info(
        &host(),
        &json!({
            "ID": "daemon-1",
            "Name": "nashost",
            "Containers": 7,
            "ContainersRunning": 5,
            "NCPU": 16,
            "MemTotal": 34359738368_u64
        }),
    )
    .unwrap();
    assert_eq!(system.daemon_id.as_deref(), Some("daemon-1"));
    assert_eq!(system.containers, 7);
    assert_eq!(system.containers_running, 5);
    assert_eq!(system.cpus, 16);

    let container = map_container_summary(
        &host(),
        &json!({
            "Id": "abc",
            "Names": ["/soma", 17],
            "State": "running",
            "Labels": {"com.example.service": "soma", "ignored": 4}
        }),
    )
    .unwrap();
    assert_eq!(container.names, vec!["/soma"]);
    assert_eq!(container.state, ContainerState::Running);
    assert_eq!(container.labels.len(), 1);
}

#[test]
fn inspect_uses_nested_state_and_config() {
    let inspect = map_container_inspect(
        &host(),
        &json!({
            "Id": "abc",
            "Name": "/soma",
            "State": {"Status": "exited", "Pid": 0, "ExitCode": 23},
            "Config": {"Labels": {"com.example.service": "soma"}},
            "RestartCount": 2
        }),
    )
    .unwrap();
    assert_eq!(inspect.state, ContainerState::Exited);
    assert_eq!(inspect.exit_code, Some(23));
    assert_eq!(inspect.restart_count, Some(2));
    assert_eq!(inspect.labels["com.example.service"], "soma");
}

#[test]
fn required_image_and_volume_identifiers_are_enforced() {
    assert!(map_image(&host(), &json!({"RepoTags": ["soma:latest"]})).is_err());
    assert!(map_volume(&host(), &json!({"Driver": "local"})).is_err());
}

#[test]
fn image_network_and_volume_fields_map_successfully() {
    let image = map_image(
        &host(),
        &json!({
            "Id": "sha256:1",
            "RepoTags": ["soma:latest"],
            "RepoDigests": ["soma@sha256:1"],
            "Created": 1,
            "Size": 2,
            "Containers": 3
        }),
    )
    .unwrap();
    assert_eq!(image.id, "sha256:1");
    assert_eq!(image.repo_tags, vec!["soma:latest"]);

    let network = map_network(
        &host(),
        &json!({
            "Id": "net1",
            "Name": "default",
            "Driver": "bridge",
            "Internal": false,
            "Attachable": true
        }),
    )
    .unwrap();
    assert_eq!(network.name.as_deref(), Some("default"));
    assert_eq!(network.internal, Some(false));

    let volume = map_volume(
        &host(),
        &json!({
            "Name": "data",
            "Driver": "local",
            "Mountpoint": "/var/lib/docker/volumes/data",
            "Scope": "local"
        }),
    )
    .unwrap();
    assert_eq!(volume.name, "data");
    assert_eq!(volume.driver, "local");
}
