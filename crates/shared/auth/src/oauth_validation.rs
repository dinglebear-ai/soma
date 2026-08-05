//! Shared validation for OAuth request credentials and correlation values.

use crate::error::AuthError;

const MIN_PKCE_LENGTH: usize = 43;
const MAX_PKCE_LENGTH: usize = 128;
const MAX_STATE_LENGTH: usize = 512;
const MIN_NATIVE_POLL_STATE_LENGTH: usize = 32;

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_nqchar(byte: u8) -> bool {
    byte == b'!' || (b'#'..=b'[').contains(&byte) || (b']'..=b'~').contains(&byte)
}

/// Validate an RFC 7636 code challenge for the S256 method.
pub(crate) fn validate_s256_code_challenge(value: &str) -> Result<(), AuthError> {
    if value.len() != MIN_PKCE_LENGTH || !value.bytes().all(is_unreserved) {
        return Err(AuthError::Validation(
            "code_challenge for S256 must be exactly 43 unreserved URI characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate an RFC 7636 code verifier before hashing it.
pub(crate) fn validate_code_verifier(value: &str) -> Result<(), AuthError> {
    if !(MIN_PKCE_LENGTH..=MAX_PKCE_LENGTH).contains(&value.len())
        || !value.bytes().all(is_unreserved)
    {
        return Err(AuthError::InvalidGrant(
            "code_verifier must contain 43 to 128 unreserved URI characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate OAuth state and, for the custom native polling flow, require it
/// to be strong enough to act as the one-time poll credential.
pub(crate) fn validate_client_state(value: &str, native_poll: bool) -> Result<(), AuthError> {
    if value.is_empty() || value.len() > MAX_STATE_LENGTH || !value.bytes().all(is_nqchar) {
        return Err(AuthError::Validation(
            "state must contain 1 to 512 visible ASCII characters excluding quote and backslash"
                .to_string(),
        ));
    }
    if native_poll
        && (value.len() < MIN_NATIVE_POLL_STATE_LENGTH || !value.bytes().all(is_unreserved))
    {
        return Err(AuthError::Validation(
            "native polling requires state to contain at least 32 unreserved URI characters"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_client_state, validate_code_verifier, validate_s256_code_challenge};

    #[test]
    fn accepts_rfc7636_values() {
        let value = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP1";
        assert_eq!(value.len(), 43);
        validate_s256_code_challenge(value).unwrap();
        validate_code_verifier(value).unwrap();
    }

    #[test]
    fn rejects_short_or_padded_pkce_values() {
        assert!(validate_s256_code_challenge("short").is_err());
        assert!(
            validate_s256_code_challenge("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO=").is_err()
        );
        assert!(validate_code_verifier("short").is_err());
    }

    #[test]
    fn native_poll_state_requires_an_unreserved_high_entropy_shape() {
        validate_client_state("abcdefghijklmnopqrstuvwxyzABCDEF", true).unwrap();
        assert!(validate_client_state("native-client-state", true).is_err());
        assert!(validate_client_state("abcdefghijklmnopqrstuvwxyzABCDE/", true).is_err());
    }

    #[test]
    fn ordinary_state_rejects_empty_controls_quote_and_backslash() {
        validate_client_state("abc-123", false).unwrap();
        for value in ["", "a\nb", r"a\b", "a\"b"] {
            assert!(validate_client_state(value, false).is_err(), "{value:?}");
        }
    }
}
