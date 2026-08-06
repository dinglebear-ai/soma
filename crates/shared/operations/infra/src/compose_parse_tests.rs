use std::path::PathBuf;

use soma_fleet::{HostEndpoint, HostId, HostRecord};

use super::*;
use crate::ComposeProjectRef;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn project() -> ComposeProjectRef {
    ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap()
}

#[test]
fn project_list_accepts_array_and_json_lines() {
    let array = r#"[{"Name":"soma","Status":"running(2)","ConfigFiles":"/srv/soma/compose.yaml"}]"#;
    let projects = parse_project_list(&host(), array).unwrap();
    assert_eq!(projects[0].name, "soma");
    assert_eq!(projects[0].host.as_str(), "devhost");
    assert_eq!(projects[0].status.as_deref(), Some("running(2)"));
    assert_eq!(
        projects[0].config_files,
        vec![PathBuf::from("/srv/soma/compose.yaml")]
    );

    let lines = "{\"Name\":\"a\"}\n{\"Name\":\"b\"}\n";
    assert_eq!(parse_project_list(&host(), lines).unwrap().len(), 2);
    assert!(parse_project_list(&host(), "not-json").is_err());
    assert!(
        parse_project_list(&host(), r#"[{"Name":"bad","ConfigFiles":"relative.yml"}]"#).is_err()
    );
}

#[test]
fn status_normalizes_compose_field_variants() {
    let raw = r#"[{"Service":"api","Name":"soma-api-1","State":"running","Health":"healthy","ExitCode":0,"Image":"soma:latest"}]"#;
    let status = parse_status(&host(), &project(), raw).unwrap();
    assert_eq!(status.project, "soma");
    assert_eq!(status.services.len(), 1);
    assert_eq!(status.services[0].service, "api");
    assert_eq!(status.services[0].exit_code, Some(0));
    assert_eq!(status.services[0].health.as_deref(), Some("healthy"));
}

#[test]
fn config_selects_stable_service_network_and_volume_fields() {
    let raw = r#"{
      "services": {
        "api": {"image":"soma:latest","profiles":["prod"]},
        "web": {"build":{"context":"./web"}}
      },
      "networks": {"default": {}},
      "volumes": {"data": {}}
    }"#;
    let config = parse_config(&host(), &project(), raw).unwrap();
    assert_eq!(config.services["api"].image.as_deref(), Some("soma:latest"));
    assert_eq!(config.services["api"].profiles, vec!["prod"]);
    assert_eq!(
        config.services["web"].build_context.as_deref(),
        Some("./web")
    );
    assert_eq!(config.networks, vec!["default"]);
    assert_eq!(config.volumes, vec!["data"]);
    assert!(parse_config(&host(), &project(), "{}").is_err());
}
