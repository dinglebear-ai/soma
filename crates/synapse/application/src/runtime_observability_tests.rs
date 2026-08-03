use super::*;

#[test]
fn journal_priority_parsing_rejects_unknown_values() {
    assert!(parse_priority("emerg").is_ok());
    assert!(parse_priority("verbose").is_err());
}
