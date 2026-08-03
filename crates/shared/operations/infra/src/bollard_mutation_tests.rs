use soma_ops::MutationSendState;

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn expired_send_is_rejected_before_polling_backend_future() {
    let cancellation = CancellationToken::new();
    let result = await_send(
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() - 1),
        &cancellation,
        async { Ok(()) },
    )
    .await;
    assert_eq!(result.unwrap_err().send_state(), MutationSendState::NotSent);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_after_send_boundary_is_conservative() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let result = await_send(
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
        &cancellation,
        std::future::pending(),
    )
    .await;
    assert_eq!(result.unwrap_err().send_state(), MutationSendState::Unknown);
}
