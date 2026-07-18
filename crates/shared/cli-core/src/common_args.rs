//! Common CLI flag-scanning primitives.
//!
//! Minimal, dependency-light helpers for hand-rolled argument parsers that
//! do not want to pull in a full parser crate. Each helper is generic over
//! the command name and flag being checked, so a consuming CLI supplies its
//! own vocabulary (command names, flag spellings) and gets consistent error
//! wording for free.
//!
//! These are not a parser framework — they scan an already-split `&[String]`
//! slice for a small, fixed set of shapes (no value, one required/optional
//! value, reject-everything). A product CLI composes them per-subcommand.

use std::error::Error;
use std::fmt;

/// An error produced while scanning CLI arguments.
///
/// Carries a fully-formatted, human-readable message. Implements
/// [`std::error::Error`] so it converts into `anyhow::Error` (or any other
/// `Error`-based error type) via `?` without an explicit `map_err`.
///
/// The message field is private so every `ArgParseError` is built through
/// this module's formatting helper, keeping wording consistent — construct
/// one only via the functions below, and read the message back with
/// [`ArgParseError::message`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgParseError(String);

impl ArgParseError {
    /// The formatted, human-readable message.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArgParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for ArgParseError {}

fn err(message: impl Into<String>) -> ArgParseError {
    ArgParseError(message.into())
}

/// Reject a subcommand that takes no arguments at all.
pub fn reject_args(args: &[String], command: &str) -> Result<(), ArgParseError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(err(format!(
            "{command} does not accept argument `{}`",
            args[0]
        )))
    }
}

/// Scan for a single boolean flag (present or absent), rejecting anything
/// else including duplicates.
pub fn parse_bool_flag(args: &[String], command: &str, flag: &str) -> Result<bool, ArgParseError> {
    let mut found = false;
    for arg in args {
        if arg == flag {
            if found {
                return Err(err(format!("{command} received duplicate {flag}")));
            }
            found = true;
        } else {
            return Err(err(format!("{command} does not accept argument `{arg}`")));
        }
    }
    Ok(found)
}

/// Scan for an optional `--flag value` pair. Returns `Ok(None)` if the flag
/// is absent, `Ok(Some(value))` if present with a value, and an error for
/// duplicates, missing values, or unexpected trailing arguments.
pub fn parse_optional_value_flag(
    args: &[String],
    command: &str,
    flag: &str,
) -> Result<Option<String>, ArgParseError> {
    match args {
        [] => Ok(None),
        [found_flag, value] if found_flag == flag => {
            if value.starts_with("--") {
                Err(err(format!("{command} requires a value after {flag}")))
            } else {
                Ok(Some(value.clone()))
            }
        }
        [found_flag] if found_flag == flag => {
            Err(err(format!("{command} requires a value after {flag}")))
        }
        [found_flag, value, rest @ ..] if found_flag == flag => {
            if value.starts_with("--") {
                Err(err(format!("{command} requires a value after {flag}")))
            } else if rest.iter().any(|arg| arg == flag) {
                Err(err(format!("{command} received duplicate {flag}")))
            } else {
                Err(err(format!(
                    "{command} does not accept argument `{}`",
                    rest[0]
                )))
            }
        }
        [unexpected, ..] => Err(err(format!(
            "{command} does not accept argument `{unexpected}`"
        ))),
    }
}

/// Scan for a `--flag value` pair the caller treats as required.
///
/// This has the same scanning behavior as [`parse_optional_value_flag`] —
/// "required" is enforced by the caller inspecting the `None` case, keeping
/// this helper's error wording identical for both required and optional
/// call sites.
pub fn parse_required_value_flag(
    args: &[String],
    command: &str,
    flag: &str,
) -> Result<Option<String>, ArgParseError> {
    parse_optional_value_flag(args, command, flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reject_args_accepts_empty() {
        reject_args(&[], "status").unwrap();
    }

    #[test]
    fn reject_args_rejects_extra() {
        let err = reject_args(&v(&["bogus"]), "status").unwrap_err();
        assert!(err.to_string().contains("status does not accept argument"));
    }

    #[test]
    fn bool_flag_found_and_absent() {
        assert!(parse_bool_flag(&v(&["--json"]), "doctor", "--json").unwrap());
        assert!(!parse_bool_flag(&[], "doctor", "--json").unwrap());
    }

    #[test]
    fn bool_flag_rejects_duplicate() {
        let err = parse_bool_flag(&v(&["--json", "--json"]), "doctor", "--json").unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn optional_value_flag_round_trips() {
        assert_eq!(
            parse_optional_value_flag(&v(&["--name", "Alice"]), "greet", "--name").unwrap(),
            Some("Alice".to_string())
        );
        assert_eq!(
            parse_optional_value_flag(&[], "greet", "--name").unwrap(),
            None
        );
    }

    #[test]
    fn optional_value_flag_requires_value() {
        let err = parse_optional_value_flag(&v(&["--name"]), "greet", "--name").unwrap_err();
        assert!(err.to_string().contains("requires a value"));
    }

    #[test]
    fn optional_value_flag_rejects_duplicate() {
        let err =
            parse_optional_value_flag(&v(&["--name", "Alice", "--name", "Bob"]), "greet", "--name")
                .unwrap_err();
        assert!(err.to_string().contains("duplicate --name"));
    }
}
