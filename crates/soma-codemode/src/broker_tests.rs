use super::broker::{code_mode_unknown_tool_hint, CodeModeBroker};
use super::host::NoopHost;

#[test]
fn broker_keeps_run_scoped_ui_capture() {
    let host = NoopHost;
    let broker = CodeModeBroker::new(Some(&host));
    assert!(broker.ui_capture.lock().unwrap().is_none());
}

#[test]
fn unknown_tool_hint_points_to_codemode_discovery() {
    assert!(code_mode_unknown_tool_hint().contains("codemode.search"));
}
