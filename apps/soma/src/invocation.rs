//! Converts the top-level CLI invocation into an execution mode.
//!
//! Mode selection stays separate from what each mode does: this module only
//! classifies `argv` into a [`Mode`]. `local.rs` runs CLI commands, `http.rs`
//! runs the HTTP server, and `stdio.rs` runs the stdio MCP transport (plan
//! section 3.1).
//!
//! [`Mode`] splits into [`ExitAction`] (help/version: print and exit, no
//! dispatch) and [`DispatchMode`] (serve/stdio/cli: run something) so the
//! caller cannot accidentally try to dispatch a help/version request — the
//! type system rules it out instead of a runtime `unreachable!()` backstop.

/// How the `soma` binary was invoked: either an early exit ([`ExitAction`])
/// or something to dispatch and run ([`DispatchMode`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Print something and exit without building any engine.
    Exit(ExitAction),
    /// Dispatch to a mode that runs until completion.
    Dispatch(DispatchMode),
}

/// `--help`/`-h`/`--version`/`-V`/`version`: print and exit immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitAction {
    /// `--help` / `-h`: print usage and exit.
    Help,
    /// `--version` / `-V` / `version`: print the binary version and exit.
    Version,
}

/// A mode that runs until completion: the HTTP server, the stdio MCP
/// transport, or a one-shot CLI command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchMode {
    /// `serve` / `serve mcp`: run the HTTP server.
    Serve,
    /// `mcp`: run the stdio MCP transport.
    Stdio,
    /// Any other subcommand: dispatch through the CLI.
    Cli,
}

impl DispatchMode {
    /// Default `RUST_LOG` level for this mode when the environment does not
    /// set one explicitly. Stdio and CLI modes stay quiet (`warn`) so JSON-RPC
    /// framing and terminal output are never corrupted by log lines; the HTTP
    /// server defaults to `info`.
    pub(crate) fn default_log_level(self) -> &'static str {
        match self {
            DispatchMode::Serve => "info",
            DispatchMode::Stdio | DispatchMode::Cli => "warn",
        }
    }
}

/// Classify `argv` (excluding `argv[0]`) into an execution [`Mode`].
pub(crate) fn resolve(args: &[String]) -> Mode {
    if matches!(args, [f] if matches!(f.as_str(), "--help" | "-h")) {
        return Mode::Exit(ExitAction::Help);
    }
    if matches!(args, [f] if matches!(f.as_str(), "--version" | "-V" | "version")) {
        return Mode::Exit(ExitAction::Version);
    }
    if is_http_server_request(args) {
        return Mode::Dispatch(DispatchMode::Serve);
    }
    if matches!(args, [c] if c == "mcp") {
        return Mode::Dispatch(DispatchMode::Stdio);
    }
    Mode::Dispatch(DispatchMode::Cli)
}

fn is_http_server_request(args: &[String]) -> bool {
    matches!(args, [c] if c == "serve") || matches!(args, [a, b] if a == "serve" && b == "mcp")
}

#[cfg(test)]
#[path = "invocation_tests.rs"]
mod tests;
