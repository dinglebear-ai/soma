//! ANSI color/style helpers.
//!
//! Every helper takes an explicit `enabled` flag rather than sampling the
//! environment itself — callers decide whether to colorize using
//! [`crate::terminal`] and pass the result in. This keeps styling decisions
//! testable and keeps a single source of truth for "should this stream be
//! colored" per invocation.

/// Wrap `text` in the given ANSI SGR `code` when `enabled`, otherwise return
/// `text` unchanged.
pub fn style(text: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Standard ANSI green (SGR 32) — conventionally "success" / "ok".
pub fn green(text: &str, enabled: bool) -> String {
    style(text, "32", enabled)
}

/// Standard ANSI red (SGR 31) — conventionally "error" / "failed".
pub fn red(text: &str, enabled: bool) -> String {
    style(text, "31", enabled)
}

/// Standard ANSI yellow (SGR 33) — conventionally "warning" / "hint".
pub fn yellow(text: &str, enabled: bool) -> String {
    style(text, "33", enabled)
}

/// Bold (SGR 1).
pub fn bold(text: &str, enabled: bool) -> String {
    style(text, "1", enabled)
}

/// Dim (SGR 2) — conventionally section rules and secondary text.
pub fn dim(text: &str, enabled: bool) -> String {
    style(text, "2", enabled)
}

/// The Aurora CLI token palette, kept as reusable shared defaults per
/// repo convention (`aurora-design-system/themes/editors/claude-code/TOKENS.md`).
///
/// These are truecolor (24-bit) hex values with an ANSI-256 fallback for
/// terminals that do not support 24-bit color. Nothing in this crate wires
/// these into a default rendering path — a consumer opts in explicitly by
/// picking a role and calling [`truecolor_fg`] or using the ANSI-256 code
/// directly.
pub mod aurora {
    /// One Aurora CLI token: a truecolor hex value plus its ANSI-256
    /// fallback code.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Token {
        pub hex: (u8, u8, u8),
        pub ansi256: u8,
    }

    pub const ACCENT: Token = Token {
        hex: (0x29, 0xb6, 0xf6),
        ansi256: 39,
    };
    pub const TERTIARY: Token = Token {
        hex: (0x67, 0xcb, 0xfa),
        ansi256: 81,
    };
    pub const PRIMARY: Token = Token {
        hex: (0xe6, 0xf4, 0xfb),
        ansi256: 255,
    };
    pub const MUTED: Token = Token {
        hex: (0xa7, 0xbc, 0xc9),
        ansi256: 250,
    };
    pub const SERVICE_NAME: Token = Token {
        hex: (0xf9, 0xa8, 0xc4),
        ansi256: 217,
    };
    pub const VIOLET: Token = Token {
        hex: (0xa7, 0x8b, 0xfa),
        ansi256: 141,
    };
    pub const BORDER: Token = Token {
        hex: (0x1d, 0x3d, 0x4e),
        ansi256: 239,
    };
    pub const INFO: Token = Token {
        hex: (0x72, 0xc8, 0xf5),
        ansi256: 117,
    };
    pub const SUCCESS: Token = Token {
        hex: (0x7d, 0xd3, 0xc7),
        ansi256: 115,
    };
    pub const WARN: Token = Token {
        hex: (0xc6, 0xa3, 0x6b),
        ansi256: 180,
    };
    pub const ERROR: Token = Token {
        hex: (0xc7, 0x84, 0x90),
        ansi256: 174,
    };
    pub const NEUTRAL: Token = Token {
        hex: (0x91, 0xa8, 0xb6),
        ansi256: 109,
    };
}

/// Wrap `text` in a 24-bit truecolor foreground escape for `token` when
/// `enabled`, otherwise return `text` unchanged.
pub fn truecolor_fg(text: &str, token: aurora::Token, enabled: bool) -> String {
    if enabled {
        let (r, g, b) = token.hex;
        format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Wrap `text` in an ANSI-256 foreground escape for `token` when `enabled`,
/// otherwise return `text` unchanged. Use as a fallback on terminals that do
/// not support 24-bit truecolor.
pub fn ansi256_fg(text: &str, token: aurora::Token, enabled: bool) -> String {
    if enabled {
        format!("\x1b[38;5;{}m{text}\x1b[0m", token.ansi256)
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_wraps_when_enabled() {
        assert_eq!(green("ok", true), "\x1b[32mok\x1b[0m");
        assert_eq!(red("bad", true), "\x1b[31mbad\x1b[0m");
        assert_eq!(yellow("hint", true), "\x1b[33mhint\x1b[0m");
        assert_eq!(bold("hi", true), "\x1b[1mhi\x1b[0m");
        assert_eq!(dim("lo", true), "\x1b[2mlo\x1b[0m");
    }

    #[test]
    fn style_passes_through_when_disabled() {
        assert_eq!(green("ok", false), "ok");
        assert_eq!(red("bad", false), "bad");
        assert_eq!(yellow("hint", false), "hint");
        assert_eq!(bold("hi", false), "hi");
        assert_eq!(dim("lo", false), "lo");
    }

    #[test]
    fn truecolor_and_ansi256_wrap_when_enabled() {
        assert_eq!(
            truecolor_fg("x", aurora::ACCENT, true),
            "\x1b[38;2;41;182;246mx\x1b[0m"
        );
        assert_eq!(
            ansi256_fg("x", aurora::ACCENT, true),
            "\x1b[38;5;39mx\x1b[0m"
        );
        assert_eq!(truecolor_fg("x", aurora::ACCENT, false), "x");
        assert_eq!(ansi256_fg("x", aurora::ACCENT, false), "x");
    }
}
