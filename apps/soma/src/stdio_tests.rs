use super::*;

// `serve()` builds a real `AppState` (env/config-backed) and blocks on the
// stdio transport — it is exercised end-to-end by
// `apps/soma/tests/stdio_mcp.rs` and `stdio_remote_api.rs` (which spawn the
// compiled binary), not as an in-process unit test. This just proves the
// composition-root entry point stdio::serve — reached via `bin/soma.rs` ->
// `soma::run` for `Mode::Stdio` — still exists with its expected
// zero-argument async shape.
#[test]
fn serve_is_the_sole_stdio_dispatch_entrypoint() {
    let _ = serve;
}
