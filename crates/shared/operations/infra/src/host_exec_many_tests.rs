use std::time::Duration;

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};
use soma_ops::{OperationId, OperationName, Timestamp};

use super::*;
use crate::{HostExecCommand, HostExecMutator};

struct Fake;

#[async_trait]
impl HostExecMutator for Fake {
    async fn exec_host(
        &self,
        host: &HostRecord,
        request: &HostExecRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<HostExecReceipt> {
        if host.id().as_str() == "error" {
            return Err(MutationFailure::new(
                MutationSendState::Unknown,
                InfraError::Docker("transport lost".into()),
            ));
        }
        let code = if host.id().as_str() == "nonzero" {
            2
        } else {
            0
        };
        Ok(HostExecReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            command: request.command(),
            args: request.args().to_vec(),
            working_dir: request.working_dir().map(ToOwned::to_owned),
            stdout: host.id().to_string(),
            stderr: String::new(),
            exit_code: Some(code),
            truncated: false,
            encoding_lossy: false,
            send_state: MutationSendState::Sent,
        })
    }
}

fn target(name: &str, path: &str) -> (HostRecord, HostExecRequest) {
    let host = HostRecord::new(HostId::new(name).unwrap(), HostEndpoint::Local);
    let request = HostExecRequest::new(
        OperationId::new(),
        OperationName::new("host.exec_many").unwrap(),
        HostExecCommand::Ls,
        vec![path.into()],
        Some(path.into()),
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap();
    (host, request)
}

#[tokio::test]
async fn fanout_preserves_order_partial_results_and_send_state() {
    let outcome = HostExecManyEngine::new(2, Duration::from_secs(1))
        .unwrap()
        .execute(
            &Fake,
            vec![
                target("ok", "/srv/a"),
                target("nonzero", "/srv/b"),
                target("error", "/srv/c"),
            ],
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.results[0].host.as_str(), "ok");
    assert_eq!(outcome.results[1].host.as_str(), "nonzero");
    assert_eq!(outcome.results[2].host.as_str(), "error");
    assert_eq!(outcome.succeeded, 1);
    assert_eq!(outcome.failed, 2);
    assert_eq!(outcome.send_state, MutationSendState::Unknown);
    assert!(!outcome.all_succeeded());
}

#[tokio::test]
async fn cancellation_before_fanout_is_not_sent() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let failure = HostExecManyEngine::new(1, Duration::from_secs(1))
        .unwrap()
        .execute(&Fake, vec![target("ok", "/srv")], cancellation)
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::NotSent);
}
