use super::safety::{safe_git_env, validate_ref};

#[test]
fn validates_refs_and_env_scrub() {
    assert!(validate_ref("main").is_ok());
    assert!(validate_ref("-bad").is_err());
    assert!(safe_git_env()
        .iter()
        .any(|(key, _)| *key == "GIT_TERMINAL_PROMPT"));
}
