use std::collections::BTreeMap;

use crate::config::UpstreamConfig;
use crate::upstream::pool::{InProcessUpstream, UpstreamPool};
use crate::upstream::UpstreamError;

#[tokio::test]
async fn task_operations_reject_unknown_upstreams() {
    let pool = UpstreamPool::default();

    let get_error = pool
        .get_task("missing", "task-one")
        .await
        .expect_err("unknown upstream must reject tasks/get");
    let update_error = pool
        .update_task("missing", "task-one", BTreeMap::new())
        .await
        .expect_err("unknown upstream must reject tasks/update");
    let cancel_error = pool
        .cancel_task("missing", "task-one")
        .await
        .expect_err("unknown upstream must reject tasks/cancel");

    for error in [get_error, update_error, cancel_error] {
        assert_eq!(
            error,
            UpstreamError::UnknownUpstream {
                upstream: "missing".to_owned(),
            }
        );
    }
}

#[tokio::test]
async fn task_operations_require_a_live_rmcp_peer() {
    let pool = UpstreamPool::default();
    pool.register_in_process(
        UpstreamConfig {
            name: "local".to_owned(),
            ..UpstreamConfig::default()
        },
        InProcessUpstream::new("local"),
    )
    .expect("register in-process upstream");

    let error = pool
        .get_task("local", "task-one")
        .await
        .expect_err("in-process upstream has no live rmcp task peer");

    assert_eq!(
        error,
        UpstreamError::Unsupported {
            upstream: "local".to_owned(),
            capability: "tasks",
        }
    );
}

#[test]
fn task_errors_preserve_operation_context() {
    let error = super::task_error("one", "tasks/update", "remote rejected input");

    assert_eq!(
        error,
        UpstreamError::LiveCall {
            upstream: "one".to_owned(),
            operation: "tasks/update",
            message: "remote rejected input".to_owned(),
        }
    );
}
