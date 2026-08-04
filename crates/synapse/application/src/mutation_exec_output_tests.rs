use super::*;

#[test]
fn fanout_text_budget_stays_on_utf8_boundaries() {
    let value = "\u{1F6F0}\u{FE0F}".repeat(100);
    let (trimmed, truncated) = truncate_utf8(&value, 17);
    assert!(truncated);
    assert!(trimmed.len() <= 17);
    assert!(trimmed.is_char_boundary(trimmed.len()));
}

#[test]
fn exec_output_uses_minus_one_when_exit_is_unknown() {
    let output = exec_output(None, String::new(), String::new(), true, false);
    assert_eq!(output["exit_code"], -1);
    assert_eq!(output["timed_out"], true);
}
