use super::*;

#[test]
fn text_and_compare_results_report_bounded_metadata() {
    assert_eq!(text(b"one\ntwo\n", false, None)["line_count"], 2);
    assert_eq!(compare(b"same", b"same", "a", "b")["equal"], true);
}
