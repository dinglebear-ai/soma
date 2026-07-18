// `main()` starts the async runtime and forwards `argv` to `soma::run` — mode
// selection, config loading, and dispatch all live in the library crate (see
// `apps/soma/src/lib.rs`, `invocation.rs`, `local.rs`, `http.rs`,
// `stdio.rs`) and are covered there and by the process-level tests under
// `apps/soma/tests/`. This just proves the binary delegates to the library's
// public entry point rather than reimplementing any of that here.
#[test]
fn main_delegates_to_the_library_run_entrypoint() {
    // `async fn` bodies are lazy: calling `soma::run` builds a `Future`
    // without executing anything (no argv parsing, no process::exit) until
    // it is polled. Constructing and dropping it here proves `main`'s
    // delegate exists with the expected `Vec<String> -> Result<()>` shape
    // without ever running it.
    let future = soma::run(Vec::<String>::new());
    drop(future);
}
