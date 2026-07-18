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
    match mode {
        ColorMode::Always => true,
        ColorMode::Plain => false,
        ColorMode::Auto => stream_is_tty && std::env::var_os("NO_COLOR").is_none(),
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
        // Save/restore so this test doesn't leak state into others sharing
        // the process (NO_COLOR is process-global).
        let previous = std::env::var_os("NO_COLOR");
        std::env::set_var("NO_COLOR", "1");
        let result = resolve_color(ColorMode::Auto, true);
        match previous {
            Some(value) => std::env::set_var("NO_COLOR", value),
            None => std::env::remove_var("NO_COLOR"),
        }
        assert!(!result, "NO_COLOR should disable Auto color even on a tty");
    }
}
