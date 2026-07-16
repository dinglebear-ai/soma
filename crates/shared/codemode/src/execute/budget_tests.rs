use serde_json::json;

use super::budget::RunBudget;
use crate::CodeModeConfig;

#[test]
fn budget_rejects_operations_over_configured_limit() {
    let config = CodeModeConfig {
        max_calls_per_run: Some(1),
        ..CodeModeConfig::default()
    };
    let mut budget = RunBudget::new(&config);

    assert!(budget.record_operation("tool call").is_ok());
    assert_eq!(
        budget.record_operation("tool call").unwrap_err().kind(),
        "budget_exceeded"
    );
}

#[test]
fn budget_caps_large_tool_results() {
    let config = CodeModeConfig {
        calltool_result_max_mib: Some(1),
        ..CodeModeConfig::default()
    };
    let budget = RunBudget::new(&config);
    let capped = budget.cap_tool_result(json!("x".repeat(2 * 1024 * 1024)));

    assert_eq!(capped["truncated"], true);
}

#[test]
fn budget_caps_logs_by_count_and_size() {
    let config = CodeModeConfig {
        max_log_entries: 1,
        max_log_bytes: 8,
        ..CodeModeConfig::default()
    };
    let budget = RunBudget::new(&config);

    assert_eq!(budget.cap_logs(vec!["abcd".into(), "efgh".into()]).len(), 2);
}
