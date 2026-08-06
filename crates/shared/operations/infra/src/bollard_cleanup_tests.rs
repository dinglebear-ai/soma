use super::*;

#[test]
fn cleanup_receipts_sort_and_clamp_backend_values() {
    let receipt = scope(
        DockerPruneTarget::Containers,
        vec!["b".into(), "a".into(), "a".into()],
        Some(-1),
    );
    assert_eq!(receipt.deleted, ["a", "b"]);
    assert_eq!(receipt.space_reclaimed, 0);
}

#[tokio::test]
async fn expired_cleanup_is_not_sent() {
    let cancellation = CancellationToken::new();
    let result = await_send(Timestamp::from_unix_millis(1), &cancellation, async {
        Ok::<_, bollard::errors::Error>(())
    })
    .await;
    assert_eq!(result.unwrap_err().send_state(), MutationSendState::NotSent);
}
