use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use soma_fleet::{HostId, HostRecord, TopologySnapshot};
use soma_infra::HostExecCommand;
use soma_ops::{
    OperationContext, OperationName, OperationPlan, PlanStep, PlannedChange, TargetKind, TargetRef,
};

use crate::runtime_params::{object, optional_str, required_str, u32_or};
use crate::{ExecutionError, SynapseMutationRuntime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ContainerExecSpec {
    pub(crate) host: String,
    pub(crate) container: String,
    pub(crate) command: Vec<String>,
    pub(crate) user: Option<String>,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HostExecSpec {
    pub(crate) host: String,
    pub(crate) command: HostExecCommand,
    pub(crate) args: Vec<String>,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) timeout_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct HostExecTargetSpec {
    pub(crate) host: String,
    pub(crate) working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HostExecManySpec {
    pub(crate) command: HostExecCommand,
    pub(crate) args: Vec<String>,
    pub(crate) targets: Vec<HostExecTargetSpec>,
    pub(crate) timeout_secs: u32,
}

pub(crate) fn exec_operation(operation: &OperationName) -> bool {
    matches!(
        operation.as_str(),
        "container.exec" | "host.exec" | "host.exec_many"
    )
}

impl SynapseMutationRuntime {
    pub(crate) async fn plan_exec(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        match operation.as_str() {
            "container.exec" => {
                let execution = container_spec(parameters)?;
                let host = self.resolve_host(&execution.host).await?;
                let target =
                    crate::mutation_runtime::container_target(&host, &execution.container)?;
                let digest = digest(&execution)?;
                one_target_plan(
                    operation,
                    context,
                    spec,
                    target,
                    host.revision().as_str(),
                    digest,
                    format!(
                        "execute {} direct arguments in container {} on {}",
                        execution.command.len(),
                        execution.container,
                        host.id()
                    ),
                )
            }
            "host.exec" => {
                let execution = host_spec(parameters)?;
                let host = self.resolve_host(&execution.host).await?;
                let target = host_target(&host)?;
                let digest = digest(&execution)?;
                one_target_plan(
                    operation,
                    context,
                    spec,
                    target,
                    host.revision().as_str(),
                    digest,
                    format!(
                        "execute allowlisted {} on host {}",
                        execution.command.as_str(),
                        host.id()
                    ),
                )
            }
            "host.exec_many" => {
                let execution = host_many_spec(parameters)?;
                let snapshot = self.ports.hosts.snapshot().await?;
                many_plan(operation, context, spec, &snapshot, &execution)
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }
}

fn one_target_plan(
    operation: &OperationName,
    context: &OperationContext,
    spec: &soma_ops::OperationSpec,
    target: TargetRef,
    topology_revision: &str,
    digest: String,
    summary: String,
) -> Result<OperationPlan, ExecutionError> {
    let change = PlannedChange::new(target.clone(), "exec", summary.clone())?
        .with_digests(Some(digest), None);
    let step = PlanStep::new(1, operation.clone(), target.clone(), summary)?;
    OperationPlan::new(
        context.operation_id().clone(),
        operation.clone(),
        target,
        spec.risk(),
        spec.reversibility(),
    )?
    .with_topology_revision(topology_revision)?
    .with_change(change)?
    .with_prerequisite("the selected execution port is configured and reachable")?
    .with_step(step)?
    .with_rollback_guidance(
        "command execution cannot be automatically rolled back; inspect captured output and reconcile any side effects manually",
    )
    .map_err(ExecutionError::from)
}

fn many_plan(
    operation: &OperationName,
    context: &OperationContext,
    spec: &soma_ops::OperationSpec,
    snapshot: &TopologySnapshot,
    execution: &HostExecManySpec,
) -> Result<OperationPlan, ExecutionError> {
    let target = TargetRef::new(TargetKind::Host, "fanout")?
        .with_revision(snapshot.revision().to_string())?;
    let mut plan = OperationPlan::new(
        context.operation_id().clone(),
        operation.clone(),
        target,
        spec.risk(),
        spec.reversibility(),
    )?
    .with_topology_revision(snapshot.revision().to_string())?
    .with_prerequisite("every selected host exists in the same immutable topology snapshot")?;
    for (index, selected) in execution.targets.iter().enumerate() {
        let id = HostId::new(&selected.host).map_err(|error| ExecutionError::InvalidParameter {
            field: "targets.host".into(),
            message: error.to_string(),
        })?;
        let host = snapshot
            .get(&id)
            .ok_or_else(|| ExecutionError::HostNotFound(selected.host.clone()))?;
        let target = host_target(host)?;
        let digest = digest(&(
            execution.command,
            &execution.args,
            selected,
            execution.timeout_secs,
        ))?;
        let summary = format!(
            "execute allowlisted {} on host {}",
            execution.command.as_str(),
            host.id()
        );
        plan = plan
            .with_change(
                PlannedChange::new(target.clone(), "exec", summary.clone())?
                    .with_digests(Some(digest), None),
            )?
            .with_step(PlanStep::new(
                u32::try_from(index + 1).expect("canonical target count is bounded"),
                operation.clone(),
                target,
                summary,
            )?)?;
    }
    plan.with_rollback_guidance(
        "do not retry the whole fanout; inspect per-target results and replan only unresolved hosts",
    )
    .map_err(ExecutionError::from)
}

pub(crate) fn container_spec(parameters: &Value) -> Result<ContainerExecSpec, ExecutionError> {
    Ok(ContainerExecSpec {
        host: required_str(parameters, "host")?.to_owned(),
        container: required_str(parameters, "container_id")?.to_owned(),
        command: required_string_array(parameters, "command")?,
        user: optional_str(parameters, "exec_user")?.map(str::to_owned),
        working_dir: optional_str(parameters, "exec_workdir")?.map(PathBuf::from),
        timeout_ms: u32_or(parameters, "exec_timeout_ms", 30_000)?,
    })
}

pub(crate) fn host_spec(parameters: &Value) -> Result<HostExecSpec, ExecutionError> {
    Ok(HostExecSpec {
        host: required_str(parameters, "host")?.to_owned(),
        command: parse_host_command(parameters)?,
        args: optional_string_array(parameters, "args")?,
        working_dir: optional_str(parameters, "path")?.map(PathBuf::from),
        timeout_secs: u32_or(parameters, "timeout_secs", 30)?,
    })
}

pub(crate) fn host_many_spec(parameters: &Value) -> Result<HostExecManySpec, ExecutionError> {
    let values = object(parameters)?
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| crate::runtime_params::invalid("targets", "expected an array"))?;
    let mut targets = values
        .iter()
        .map(|value| {
            let object = value.as_object().ok_or_else(|| {
                crate::runtime_params::invalid("targets", "target must be an object")
            })?;
            let host = object
                .get("host")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    crate::runtime_params::invalid("targets.host", "required string is missing")
                })?
                .to_owned();
            let working_dir = match object.get("path") {
                None | Some(Value::Null) => None,
                Some(Value::String(path)) => Some(PathBuf::from(path)),
                Some(_) => {
                    return Err(crate::runtime_params::invalid(
                        "targets.path",
                        "expected a string",
                    ));
                }
            };
            Ok(HostExecTargetSpec { host, working_dir })
        })
        .collect::<Result<Vec<_>, ExecutionError>>()?;
    targets.sort();
    if targets.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(crate::runtime_params::invalid(
            "targets",
            "duplicate host/path targets are not allowed",
        ));
    }
    Ok(HostExecManySpec {
        command: parse_host_command(parameters)?,
        args: optional_string_array(parameters, "args")?,
        targets,
        timeout_secs: u32_or(parameters, "timeout_secs", 30)?,
    })
}

fn parse_host_command(parameters: &Value) -> Result<HostExecCommand, ExecutionError> {
    HostExecCommand::parse(required_str(parameters, "command")?).map_err(ExecutionError::from)
}

fn required_string_array(parameters: &Value, field: &str) -> Result<Vec<String>, ExecutionError> {
    let value = object(parameters)?
        .get(field)
        .ok_or_else(|| crate::runtime_params::invalid(field, "required array is missing"))?;
    string_array(value, field)
}

fn optional_string_array(parameters: &Value, field: &str) -> Result<Vec<String>, ExecutionError> {
    match object(parameters)?.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(value) => string_array(value, field),
    }
}

fn string_array(value: &Value, field: &str) -> Result<Vec<String>, ExecutionError> {
    value
        .as_array()
        .ok_or_else(|| crate::runtime_params::invalid(field, "expected an array"))?
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                crate::runtime_params::invalid(field, "array values must be strings")
            })
        })
        .collect()
}

pub(crate) fn host_target(host: &HostRecord) -> Result<TargetRef, ExecutionError> {
    TargetRef::new(TargetKind::Host, host.id().to_string())?
        .with_host(host.id().to_string())?
        .with_revision(host.revision().to_string())
        .map_err(ExecutionError::from)
}

pub(crate) fn digest<T: Serialize>(value: &T) -> Result<String, ExecutionError> {
    let encoded = serde_json::to_vec(value).map_err(|error| ExecutionError::InvalidParameter {
        field: "execution".into(),
        message: error.to_string(),
    })?;
    Ok(crate::runtime_result::digest(&encoded))
}

#[cfg(test)]
#[path = "mutation_exec_tests.rs"]
mod tests;
