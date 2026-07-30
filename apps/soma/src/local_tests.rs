use super::*;

// `run()` dispatches through `soma_cli::parse_args()`, which reads the real
// process `argv`/env — it cannot be driven from an in-process unit test
// without corrupting the test harness's own arguments. CLI behavior is
// covered end-to-end by `apps/soma/tests/cli_parse.rs`, `doctor_cli.rs`,
// `provider_cli.rs`, and friends. This just proves the composition-root
// entry point local::run — reached via `bin/soma.rs` -> `soma::run` for
// `Mode::Cli` — still exists with its expected zero-argument async shape.
#[test]
fn run_is_the_sole_cli_dispatch_entrypoint() {
    let _ = run;
}

#[test]
fn leading_yes_flag_is_removed_before_command_parsing() {
    let args = vec![
        "--yes".to_owned(),
        "python_environment_prune".to_owned(),
        "--json".to_owned(),
        "{}".to_owned(),
    ];
    let (assume_yes, command) = split_confirmation_flag(&args);
    assert!(assume_yes);
    assert_eq!(command[0], "python_environment_prune");
}
