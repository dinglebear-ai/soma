//! Integration tests for `soma::run`, the library entry point
//! `apps/soma/src/bin/soma.rs` forwards `argv` to.
//!
//! Before PR 18 this dispatch logic (help/version early-exit, mode
//! classification, dispatch) lived inline in `bin/soma.rs`'s `main()` and
//! was untestable in-process. `run()` is now `pub async fn` in the library
//! crate, so the early-exit branches are directly testable without spawning
//! the compiled binary. (The `Serve`/`Stdio`/`Cli` dispatch branches run
//! until completion or touch real process state — those stay covered by the
//! existing subprocess suites: `soma_serve.rs`, `stdio_mcp.rs`,
//! `cli_parse.rs`, and friends.)

#[tokio::test]
async fn help_flag_returns_ok_without_dispatching() {
    let result = soma::run(["--help".to_string()]).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn short_help_flag_returns_ok_without_dispatching() {
    let result = soma::run(["-h".to_string()]).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn version_flag_returns_ok_without_dispatching() {
    let result = soma::run(["--version".to_string()]).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn short_version_flag_returns_ok_without_dispatching() {
    let result = soma::run(["-V".to_string()]).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn bare_version_subcommand_returns_ok_without_dispatching() {
    let result = soma::run(["version".to_string()]).await;
    assert!(result.is_ok(), "{result:?}");
}
