//! Terminal capability detection and color policy.
//!
//! These helpers decide *whether* a stream should receive ANSI styling or
//! interactive behavior (progress lines, prompts). They do not decide *what*
//! to print — see [`crate::color`] for that.

use std::io::IsTerminal;

/// Explicit color policy a CLI can expose through a `--color` flag.
///
/// `Auto` defers to [`ColorMode::Auto`]'s terminal/`NO_COLOR` detection,
/// `Always` forces styling on regardless of terminal state, and `Plain`
/// forces styling off. This mirrors the common `--color=auto|always|never`
/// convention used across CLIs (and the Aurora CLI token conventions, which
/// name the "off" state `plain`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Plain,
}

impl ColorMode {
    /// Parse a `--color` flag value. Accepts `auto`, `always`, and
    /// `plain`/`never` (both spellings are common in the wild).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "plain" | "never" => Some(Self::Plain),
            _ => None,
        }
    }
}

/// Resolve whether color should be enabled for a stream, given an explicit
/// [`ColorMode`] policy and whether that stream is a TTY.
///
/// `Auto` also honors the `NO_COLOR` convention (<https://no-color.org>):
/// any non-empty or empty `NO_COLOR` environment variable disables color.
pub fn resolve_color(mode: ColorMode, stream_is_tty: bool) -> bool {
    resolve_color_inner(mode, stream_is_tty, std::env::var_os("NO_COLOR").is_some())
}

/// Pure policy core of [`resolve_color`], with the `NO_COLOR` lookup lifted
/// out into the `no_color_set` parameter.
///
/// Splitting the environment read from the decision keeps the unit tests
/// free of process-global mutation: `std::env::set_var` is `unsafe` as of
/// Rust 2024, and this crate is `#![forbid(unsafe_code)]`, so a test that
/// toggled `NO_COLOR` in-process could not compile. Passing the flag in also
/// makes the test deterministic under the parallel test harness, where a
/// sibling test mutating the same variable could otherwise race.
fn resolve_color_inner(mode: ColorMode, stream_is_tty: bool, no_color_set: bool) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Plain => false,
        ColorMode::Auto => stream_is_tty && !no_color_set,
    }
}

/// Whether stdin is attached to an interactive terminal.
pub fn is_stdin_terminal() -> bool {
    std::io::stdin().is_terminal()
}

/// Whether stdout is attached to an interactive terminal.
pub fn is_stdout_terminal() -> bool {
    std::io::stdout().is_terminal()
}

/// Whether stderr is attached to an interactive terminal.
pub fn is_stderr_terminal() -> bool {
    std::io::stderr().is_terminal()
}

/// Whether stderr output should be colorized under the `Auto` policy: a TTY
/// and no `NO_COLOR` override.
pub fn stderr_supports_color() -> bool {
    resolve_color(ColorMode::Auto, is_stderr_terminal())
}

/// Whether stdout output should be colorized under the `Auto` policy: a TTY
/// and no `NO_COLOR` override.
pub fn stdout_supports_color() -> bool {
    resolve_color(ColorMode::Auto, is_stdout_terminal())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_known_values() {
        assert_eq!(ColorMode::parse("auto"), Some(ColorMode::Auto));
        assert_eq!(ColorMode::parse("always"), Some(ColorMode::Always));
        assert_eq!(ColorMode::parse("plain"), Some(ColorMode::Plain));
        assert_eq!(ColorMode::parse("never"), Some(ColorMode::Plain));
        assert_eq!(ColorMode::parse("bogus"), None);
    }

    #[test]
    fn always_and_plain_ignore_tty_state() {
        assert!(resolve_color(ColorMode::Always, false));
        assert!(!resolve_color(ColorMode::Plain, true));
    }

    #[test]
    fn auto_requires_tty() {
        assert!(!resolve_color(ColorMode::Auto, false));
    }

    #[test]
    fn auto_respects_no_color_even_on_a_tty() {
        // Drive the policy core directly rather than mutating the real
        // process environment: `set_var` is `unsafe` in Rust 2024 and this
        // crate is `#![forbid(unsafe_code)]`.
        assert!(
            !resolve_color_inner(ColorMode::Auto, true, true),
            "NO_COLOR should disable Auto color even on a tty"
        );
        assert!(
            resolve_color_inner(ColorMode::Auto, true, false),
            "Auto color should stay enabled on a tty without NO_COLOR"
        );
    }
}
