use super::*;

#[tokio::test]
async fn signal_future_stays_pending_without_a_signal() {
    // No Ctrl+C/SIGTERM was sent, so the shutdown future must not resolve on
    // its own — a regression here (e.g. an accidentally-ready future) would
    // make the HTTP server shut down immediately after starting.
    let outcome = tokio::time::timeout(std::time::Duration::from_millis(20), signal()).await;
    assert!(
        outcome.is_err(),
        "shutdown signal resolved without a signal being sent"
    );
}
