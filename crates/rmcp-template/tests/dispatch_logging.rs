//! Regression test for the unified dispatch logging contract.
//!
//! `rtemplate_service::dispatch_action` is the single seam every surface (MCP,
//! REST, CLI) routes through, and it must emit one structured log line per
//! action with `surface`, `action`, and `outcome` fields. This test captures
//! the tracing output and asserts those fields are present so the observability
//! contract cannot silently regress.

use rtemplate_contracts::actions::ExampleAction;
use rtemplate_test_support::{tracing_test_lock, SharedBuf};

// The capture lock is intentionally held across the await: this is a
// single-threaded test (current_thread) whose whole purpose is to serialize the
// thread-local default subscriber for the duration of one dispatch.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn dispatch_action_emits_structured_log() {
    // Serialize against other tracing-capture tests; current_thread keeps the
    // awaited dispatch on the thread whose default subscriber we set.
    let _lock = tracing_test_lock();

    let buf = SharedBuf::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.writer())
        .with_ansi(false)
        .without_time()
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);

    let state = rmcp_template::testing::loopback_state();
    let result =
        rtemplate_service::dispatch_action(&state.service, &ExampleAction::Status, "rest").await;
    assert!(
        result.is_ok(),
        "status dispatch should succeed in stub mode"
    );

    drop(guard);

    let logs = buf.contents();
    assert!(logs.contains("action dispatched"), "logs were: {logs}");
    assert!(logs.contains("surface"), "missing surface field: {logs}");
    assert!(logs.contains("rest"), "missing surface value: {logs}");
    assert!(logs.contains("status"), "missing action value: {logs}");
    assert!(logs.contains("outcome"), "missing outcome field: {logs}");
}
