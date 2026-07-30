//! Stdio entrypoint for the isolated Soma Code Mode runner.

fn main() {
    if let Err(error) = soma_codemode::run_code_mode_runner_stdio_blocking() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
