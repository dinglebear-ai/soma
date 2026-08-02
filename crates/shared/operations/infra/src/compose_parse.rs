#[cfg(any(feature = "process-driver", test))]
use std::collections::BTreeMap;
use std::path::{Component, PathBuf};

#[cfg(any(feature = "process-driver", test))]
use serde_json::Value;
#[cfg(any(feature = "process-driver", test))]
use soma_fleet::HostRecord;

#[cfg(any(feature = "process-driver", test))]
use crate::{
    ComposeConfig, ComposeProject, ComposeProjectRef, ComposeServiceConfig, ComposeServiceStatus,
    ComposeStatus,
};
use crate::{InfraError, InfraResult};

pub(crate) fn validate_project_name(value: &str) -> InfraResult<()> {
    validate_name("project", value)
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn validate_service(service: &str) -> InfraResult<()> {
    validate_name("service", service)
}

pub(crate) fn validate_absolute_path(path: PathBuf) -> InfraResult<PathBuf> {
    if !path.is_absolute()
        || path.to_str().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        Err(InfraError::InvalidRequest {
            domain: "compose",
            message: format!(
                "config path must be absolute and normalized: {}",
                path.display()
            ),
        })
    } else {
        Ok(path)
    }
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn parse_project_list(host: &HostRecord, raw: &str) -> InfraResult<Vec<ComposeProject>> {
    let records = parse_records(raw, "Compose project list")?;
    records
        .into_iter()
        .map(|value| {
            let object = value
                .as_object()
                .ok_or_else(|| parse_error("project row is not an object"))?;
            let name = string_field(object, &["Name", "name"])
                .ok_or_else(|| parse_error("project row has no name"))?;
            validate_project_name(&name)?;
            let status = string_field(object, &["Status", "status"]);
            let config_files = string_field(object, &["ConfigFiles", "config_files"])
                .map(|files| {
                    files
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(PathBuf::from)
                        .map(validate_absolute_path)
                        .collect::<InfraResult<Vec<_>>>()
                })
                .transpose()?
                .unwrap_or_default();
            Ok(ComposeProject {
                host: host.id().clone(),
                topology_revision: host.revision().clone(),
                name,
                status,
                config_files,
            })
        })
        .collect()
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn parse_status(
    host: &HostRecord,
    project: &ComposeProjectRef,
    raw: &str,
) -> InfraResult<ComposeStatus> {
    let records = parse_records(raw, "Compose status")?;
    let services = records
        .into_iter()
        .map(|value| {
            let object = value
                .as_object()
                .ok_or_else(|| parse_error("service row is not an object"))?;
            let service = string_field(object, &["Service", "service", "Name", "name"])
                .ok_or_else(|| parse_error("service row has no service name"))?;
            validate_service(&service)?;
            Ok(ComposeServiceStatus {
                service,
                container_name: string_field(object, &["Name", "name"]),
                state: string_field(object, &["State", "state"]),
                health: string_field(object, &["Health", "health"]),
                exit_code: integer_field(object, &["ExitCode", "exit_code"]),
                image: string_field(object, &["Image", "image"]),
            })
        })
        .collect::<InfraResult<Vec<_>>>()?;
    Ok(ComposeStatus {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        project: project.name().to_owned(),
        services,
    })
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn parse_config(
    host: &HostRecord,
    project: &ComposeProjectRef,
    raw: &str,
) -> InfraResult<ComposeConfig> {
    let root: Value = serde_json::from_str(raw)
        .map_err(|error| parse_error(&format!("invalid config JSON: {error}")))?;
    let object = root
        .as_object()
        .ok_or_else(|| parse_error("config root is not an object"))?;
    let services_value = object
        .get("services")
        .and_then(Value::as_object)
        .ok_or_else(|| parse_error("config has no services object"))?;
    let mut services = BTreeMap::new();
    for (name, value) in services_value {
        validate_service(name)?;
        let service = value
            .as_object()
            .ok_or_else(|| parse_error("service config is not an object"))?;
        let build_context = service.get("build").and_then(|build| {
            build.as_str().map(str::to_owned).or_else(|| {
                build
                    .as_object()
                    .and_then(|value| value.get("context"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
        });
        let profiles = service
            .get("profiles")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        services.insert(
            name.clone(),
            ComposeServiceConfig {
                image: service
                    .get("image")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                build_context,
                profiles,
            },
        );
    }
    let names = |field: &str| -> Vec<String> {
        object
            .get(field)
            .and_then(Value::as_object)
            .map(|values| values.keys().cloned().collect())
            .unwrap_or_default()
    };
    Ok(ComposeConfig {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        project: project.name().to_owned(),
        services,
        networks: names("networks"),
        volumes: names("volumes"),
    })
}

#[cfg(any(feature = "process-driver", test))]
fn parse_records(raw: &str, label: &str) -> InfraResult<Vec<Value>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(values) = serde_json::from_str::<Vec<Value>>(trimmed) {
        return Ok(values);
    }
    trimmed
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| parse_error(&format!("{label} JSON line: {error}")))
        })
        .collect()
}

#[cfg(any(feature = "process-driver", test))]
fn string_field(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(Value::as_str).map(str::to_owned))
}

#[cfg(any(feature = "process-driver", test))]
fn integer_field(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| {
        let value = object.get(*name)?;
        value.as_i64().or_else(|| value.as_str()?.parse().ok())
    })
}

fn validate_name(kind: &'static str, value: &str) -> InfraResult<()> {
    let mut chars = value.chars();
    if value.len() > 256
        || !chars
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        || !chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
    {
        return Err(InfraError::InvalidRequest {
            domain: "compose",
            message: format!("invalid {kind} name: {value:?}"),
        });
    }
    Ok(())
}

#[cfg(any(feature = "process-driver", test))]
fn parse_error(message: &str) -> InfraError {
    InfraError::Parse {
        domain: "compose",
        message: message.to_owned(),
    }
}

#[cfg(test)]
#[path = "compose_tests.rs"]
mod tests;
