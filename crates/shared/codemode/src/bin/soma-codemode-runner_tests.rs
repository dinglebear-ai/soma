#[test]
fn runner_binary_uses_public_library_entrypoint() {
    let entrypoint: fn() -> Result<(), String> = soma_codemode::run_code_mode_runner_stdio_blocking;
    let _ = entrypoint;
}
