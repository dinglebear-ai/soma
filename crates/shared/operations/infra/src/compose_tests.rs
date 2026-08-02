use std::path::{Path, PathBuf};

use soma_fleet::{HostEndpoint, HostId};

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

fn project() -> ComposeProjectRef {
    ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap()
}

#[test]
fn project_references_are_closed_and_normalized() {
    let project = project();
    assert_eq!(project.name(), "soma");
    assert_eq!(project.config_file(), Path::new("/srv/soma/compose.yaml"));
    assert!(ComposeProjectRef::new("bad name", "/tmp/compose.yml").is_err());
    assert!(ComposeProjectRef::new("soma", "relative.yml").is_err());
    assert!(ComposeProjectRef::new("soma", "/srv/../etc/passwd").is_err());
    assert!(validate_service("api_1.web").is_ok());
    assert!(validate_service("bad service").is_err());
}

#[test]
fn project_list_accepts_array_and_json_lines() {
    let array = r#"[{"Name":"soma","Status":"running(2)","ConfigFiles":"/srv/soma/compose.yaml"}]"#;
    let projects = parse_project_list(&host(), array).unwrap();
    assert_eq!(projects[0].name, "soma");
    assert_eq!(projects[0].host.as_str(), "dookie");
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

#[cfg(unix)]
#[test]
fn project_references_reject_non_utf8_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let mut bytes = b"/tmp/compose-".to_vec();
    bytes.push(0xff);
    assert!(ComposeProjectRef::new("soma", PathBuf::from(OsString::from_vec(bytes))).is_err());
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
