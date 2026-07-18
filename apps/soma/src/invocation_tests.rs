use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn explicit_serve_requests_enter_server_mode() {
    assert_eq!(resolve(&args(&[])), Mode::Dispatch(DispatchMode::Cli));
    assert_eq!(
        resolve(&args(&["serve"])),
        Mode::Dispatch(DispatchMode::Serve)
    );
    assert_eq!(
        resolve(&args(&["serve", "mcp"])),
        Mode::Dispatch(DispatchMode::Serve)
    );
}

#[test]
fn local_cli_and_stdio_requests_stay_in_local_binary() {
    assert_eq!(
        resolve(&args(&["doctor"])),
        Mode::Dispatch(DispatchMode::Cli)
    );
    assert_eq!(
        resolve(&args(&["mcp"])),
        Mode::Dispatch(DispatchMode::Stdio)
    );
    assert_eq!(
        resolve(&args(&["setup", "plugin-hook"])),
        Mode::Dispatch(DispatchMode::Cli)
    );
}

#[test]
fn help_and_version_flags_are_recognized() {
    assert_eq!(resolve(&args(&["--help"])), Mode::Exit(ExitAction::Help));
    assert_eq!(resolve(&args(&["-h"])), Mode::Exit(ExitAction::Help));
    assert_eq!(
        resolve(&args(&["--version"])),
        Mode::Exit(ExitAction::Version)
    );
    assert_eq!(resolve(&args(&["-V"])), Mode::Exit(ExitAction::Version));
    assert_eq!(
        resolve(&args(&["version"])),
        Mode::Exit(ExitAction::Version)
    );
}

#[test]
fn local_and_stdio_modes_default_to_quiet_logging() {
    assert_eq!(DispatchMode::Cli.default_log_level(), "warn");
    assert_eq!(DispatchMode::Stdio.default_log_level(), "warn");
}

#[test]
fn http_server_mode_defaults_to_info_logging() {
    assert_eq!(DispatchMode::Serve.default_log_level(), "info");
}

// `resolve()` only special-cases the exact argv shapes documented on `Mode`'s
// variants; anything else — including a mistyped extra argument after a
// recognized keyword — falls through to `DispatchMode::Cli` and lets the CLI
// parser (`soma-cli`) reject it with a real error instead of silently
// running the wrong mode.
#[test]
fn unrecognized_argument_shapes_fall_back_to_cli_dispatch() {
    assert_eq!(
        resolve(&args(&["serve", "mcp", "extra"])),
        Mode::Dispatch(DispatchMode::Cli)
    );
    assert_eq!(
        resolve(&args(&["mcp", "extra-arg"])),
        Mode::Dispatch(DispatchMode::Cli)
    );
    assert_eq!(
        resolve(&args(&["--help", "extra"])),
        Mode::Dispatch(DispatchMode::Cli)
    );
}
