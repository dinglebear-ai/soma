use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use soma_fleet::{CommandExecutor, CommandRequest, HostId, HostRecord};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    HostExecMutator, HostExecPolicy, HostExecReceipt, HostExecRequest, InfraError, MutationFailure,
    MutationResult,
};

const PY_BOOTSTRAP: &str =
    "import base64,sys;exec(compile(base64.b64decode(sys.argv[1]),'<soma-host-exec>','exec'))";
const BOUND_EXEC_SOURCE: &str = r#"import json, os, sys
command = sys.argv[2]
cwd = None if sys.argv[3] == 'null' else sys.argv[3]
indices = json.loads(sys.argv[4])
root_count = int(sys.argv[5])
roots = sys.argv[6:6 + root_count]
argv = sys.argv[6 + root_count:]
fds = []
def parts(path):
    return [part for part in path.split('/') if part]
def choose(path):
    matches = [root for root in roots if path == root or root == '/' or path.startswith(root.rstrip('/') + '/')]
    if not matches:
        raise PermissionError('path outside configured roots')
    root = max(matches, key=lambda value: len(parts(value)))
    relative = path[len(root):].lstrip('/') if root != '/' else path.lstrip('/')
    return root, relative
def bind(path):
    root, relative = choose(path)
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    for part in parts(root) + parts(relative):
        next_fd = os.open(part, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
        os.close(fd)
        fd = next_fd
    os.set_inheritable(fd, True)
    fds.append(fd)
    return fd
for index in indices:
    argv[index] = '/proc/self/fd/' + str(bind(argv[index]))
if cwd is not None:
    os.fchdir(bind(cwd))
os.execvp(command, [command] + argv)
"#;

/// Process-backed host command driver with explicit per-host read roots.
pub struct CommandHostExec {
    executor: Arc<dyn CommandExecutor>,
    policies: BTreeMap<HostId, HostExecPolicy>,
}

impl CommandHostExec {
    /// Creates a driver with no admitted hosts.
    #[must_use]
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self {
            executor,
            policies: BTreeMap::new(),
        }
    }

    /// Adds or replaces the execution policy for one host identity.
    #[must_use]
    pub fn with_policy(mut self, host: HostId, policy: HostExecPolicy) -> Self {
        self.policies.insert(host, policy);
        self
    }
}

#[async_trait]
impl HostExecMutator for CommandHostExec {
    async fn exec_host(
        &self,
        host: &HostRecord,
        request: &HostExecRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<HostExecReceipt> {
        ensure_admitted(request.deadline(), cancellation)?;
        let policy = self.policies.get(host.id()).ok_or_else(|| {
            MutationFailure::new(
                MutationSendState::NotSent,
                InfraError::InvalidRequest {
                    domain: "host-exec",
                    message: format!("host execution is disabled for {}", host.id()),
                },
            )
        })?;
        let plan = policy
            .launcher_plan(request)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let source = base64::engine::general_purpose::STANDARD.encode(BOUND_EXEC_SOURCE);
        let indices = serde_json::to_string(&plan.path_indices).map_err(|error| {
            MutationFailure::new(
                MutationSendState::NotSent,
                InfraError::Parse {
                    domain: "host-exec",
                    message: error.to_string(),
                },
            )
        })?;
        let mut args = vec![
            "-c".to_owned(),
            PY_BOOTSTRAP.to_owned(),
            source,
            request.command().as_str().to_owned(),
            plan.working_dir.unwrap_or_else(|| "null".into()),
            indices,
            plan.roots.len().to_string(),
        ];
        args.extend(plan.roots);
        args.extend(request.args().iter().cloned());
        let stdout_limit = request.max_stdout_bytes();
        let stderr_limit = request.max_stderr_bytes();
        let command = CommandRequest::new("python3", args, request.deadline())
            .map_err(soma_fleet::FleetError::from)
            .and_then(|command| {
                command
                    .with_output_limits(stdout_limit, stderr_limit)
                    .map_err(soma_fleet::FleetError::from)
            })
            .map_err(|error| {
                MutationFailure::new(MutationSendState::NotSent, InfraError::from(error))
            })?;
        let output = self
            .executor
            .execute(host, &command, cancellation)
            .await
            .map_err(|error| {
                MutationFailure::new(MutationSendState::Unknown, InfraError::from(error))
            })?;
        let stdout_lossy = String::from_utf8_lossy(output.stdout());
        let stderr_lossy = String::from_utf8_lossy(output.stderr());
        Ok(HostExecReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            command: request.command(),
            args: request.args().to_vec(),
            working_dir: request.working_dir().map(ToOwned::to_owned),
            stdout: stdout_lossy.into_owned(),
            stderr: stderr_lossy.into_owned(),
            exit_code: output.exit_code(),
            truncated: output.truncated(),
            encoding_lossy: std::str::from_utf8(output.stdout()).is_err()
                || std::str::from_utf8(output.stderr()).is_err(),
            send_state: MutationSendState::Sent,
        })
    }
}

fn ensure_admitted(deadline: Timestamp, cancellation: &CancellationToken) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    if deadline <= Timestamp::now() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "process_host_exec_tests.rs"]
mod tests;
