use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn release_options_default_to_pr_head() {
    let options = ReleaseCommandOptions::parse(&[]).unwrap();
    assert_eq!(options.base, None);
    assert_eq!(options.head, "HEAD");
    assert_eq!(options.mode, release_versions::GateMode::Pr);
    assert!(!options.json);
}

#[test]
fn release_options_parse_all_flags() {
    let options = ReleaseCommandOptions::parse(&args(&[
        "--base",
        "origin/main",
        "--head",
        "feature",
        "--mode",
        "main",
        "--json",
    ]))
    .unwrap();
    assert_eq!(options.base.as_deref(), Some("origin/main"));
    assert_eq!(options.head, "feature");
    assert_eq!(options.mode, release_versions::GateMode::Main);
    assert!(options.json);
}

#[test]
fn invalid_release_mode_is_rejected() {
    assert!(parse_gate_mode("ship-it").is_err());
}
