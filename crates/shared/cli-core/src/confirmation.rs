//! Confirmation I/O primitives.
//!
//! This module owns the mechanics of asking a human to confirm an action by
//! typing a specific token back — it does not decide *which* operations
//! require confirmation. That is business/product confirmation policy and
//! belongs in the consuming application layer, never here.

use std::io::{self, BufRead, Write};

/// Write `prompt` to `writer`, flush it, read one line from `reader`, and
/// report whether the trimmed input equals `expected`.
///
/// Returns `Ok(false)` (not an error) when the input does not match —
/// callers decide how to react to a declined confirmation (abort, retry,
/// exit code) since that is product policy.
pub fn confirm_typed<R, W>(
    writer: &mut W,
    reader: &mut R,
    prompt: &str,
    expected: &str,
) -> io::Result<bool>
where
    R: BufRead,
    W: Write,
{
    write!(writer, "{prompt}")?;
    writer.flush()?;

    let mut input = String::new();
    reader.read_line(&mut input)?;
    Ok(input.trim() == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_typed_writes_prompt_and_matches_input() {
        let mut input = io::Cursor::new(b"yes\n".to_vec());
        let mut output = Vec::new();
        let confirmed = confirm_typed(&mut output, &mut input, "Type yes: ", "yes").unwrap();
        assert!(confirmed);
        assert_eq!(String::from_utf8(output).unwrap(), "Type yes: ");
    }

    #[test]
    fn confirm_typed_reports_mismatch_without_erroring() {
        let mut input = io::Cursor::new(b"nope\n".to_vec());
        let mut output = Vec::new();
        let confirmed = confirm_typed(&mut output, &mut input, "Type yes: ", "yes").unwrap();
        assert!(!confirmed);
    }

    #[test]
    fn confirm_typed_trims_trailing_whitespace() {
        let mut input = io::Cursor::new(b"yes  \r\n".to_vec());
        let mut output = Vec::new();
        let confirmed = confirm_typed(&mut output, &mut input, "Type yes: ", "yes").unwrap();
        assert!(confirmed);
    }

    #[test]
    fn confirm_typed_treats_closed_stdin_as_a_non_match() {
        // An empty reader mimics closed/EOF stdin: `read_line` returns
        // `Ok(0)` with `input` left empty, which trims to `""` — never
        // equal to a non-empty `expected` token, so this fails closed
        // (`Ok(false)`) rather than erroring or matching.
        let mut input = io::Cursor::new(Vec::new());
        let mut output = Vec::new();
        let confirmed = confirm_typed(&mut output, &mut input, "Type yes: ", "yes").unwrap();
        assert!(!confirmed);
    }
}
