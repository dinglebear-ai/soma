use serde_json::json;
use soma_fleet::{HostEndpoint, SshEndpoint};

use crate::ContainerState;

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

#[test]
fn system_info_mapping_is_host_and_revision_bound() {
    let value = json!({
        "ID": "daemon-1",
        "Name": "dookie",
        "ServerVersion": "28.0",
        "OperatingSystem": "Linux",
        "Architecture": "x86_64",
        "KernelVersion": "7.0",
        "Driver": "overlay2",
        "Containers": 4,
        "ContainersRunning": 2,
        "ContainersPaused": 1,
        "ContainersStopped": 1,
        "Images": 10,
        "NCPU": 8,
        "MemTotal": 1234
    });
    let info = map_system_info(&host(), &value).unwrap();
    assert_eq!(info.daemon_id.as_deref(), Some("daemon-1"));
    assert_eq!(info.containers_running, 2);
    assert_eq!(info.cpus, 8);
    assert_eq!(info.host.as_str(), "dookie");
}

#[test]
fn container_image_network_and_volume_mappers_are_neutral() {
    let container = map_container_summary(
        &host(),
        &json!({
            "Id":"abc","Names":["/api"],"Image":"soma:latest","ImageID":"sha256:1",
            "Command":"/app","Created":42,"State":"running","Status":"Up","Labels":{"app":"soma"}
        }),
    )
    .unwrap();
    assert_eq!(container.state, ContainerState::Running);
    assert_eq!(container.labels["app"], "soma");

    let inspect = map_container_inspect(
        &host(),
        &json!({
            "Id":"abc","Name":"/api","Created":"now","Path":"/app","Args":["serve"],
            "Image":"sha256:1","RestartCount":2,
            "State":{"Status":"exited","Pid":0,"ExitCode":17},
            "Config":{"Labels":{"app":"soma"}}
        }),
    )
    .unwrap();
    assert_eq!(inspect.state, ContainerState::Exited);
    assert_eq!(inspect.exit_code, Some(17));

    let image = map_image(
        &host(),
        &json!({"Id":"sha256:1","RepoTags":["soma:latest"],"RepoDigests":[],"Created":1,"Size":2,"Containers":3}),
    )
    .unwrap();
    assert_eq!(image.id, "sha256:1");

    let network = map_network(
        &host(),
        &json!({"Id":"net1","Name":"default","Driver":"bridge","Internal":false,"Attachable":true}),
    )
    .unwrap();
    assert_eq!(network.name.as_deref(), Some("default"));

    let volume = map_volume(
        &host(),
        &json!({"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data","Scope":"local"}),
    )
    .unwrap();
    assert_eq!(volume.name, "data");
}

#[test]
fn local_connector_rejects_remote_targets_before_socket_access() {
    let remote = HostRecord::new(
        HostId::new("remote").unwrap(),
        HostEndpoint::Ssh(SshEndpoint::new("remote").unwrap()),
    );
    assert!(matches!(
        BollardReadClient::connect_local(&remote, None),
        Err(InfraError::UnsupportedTarget { .. })
    ));
}

#[test]
fn filters_and_identifiers_are_bounded() {
    assert!(validate_filter("").is_err());
    assert!(validate_filter("app=soma").is_ok());
    assert!(validate_identifier("container", "").is_err());
    assert!(validate_identifier("container", "api").is_ok());
}
