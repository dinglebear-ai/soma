//! chacha20poly1305 AEAD wrapper for token-at-rest encryption.
//!
//! The cipher operations live in the shared crate-internal `aead` core (one
//! implementation for both at-rest stacks); this module owns key loading
//! from `{PREFIX}_OAUTH_ENCRYPTION_KEY` and the upstream error taxonomy.
//!
//! Every `seal()` call generates a **fresh random 12-byte nonce**. Callers
//! MUST store the returned nonce alongside the ciphertext and MUST NOT reuse
//! it. The upsert path in `store.rs` always replaces the stored nonce with
//! the one returned by `seal()`.

use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::{self, AeadError};

/// A loaded 32-byte encryption key ready for `seal` / `open`.  Wiped from
/// memory on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct EncryptionKey([u8; 32]);

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncryptionKey(<redacted>)")
    }
}

/// Errors from encryption/decryption operations.
#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid key: {0}")]
    InvalidKey(String),
    #[error("encryption failed")]
    EncryptionFailed,
}

impl From<AeadError> for EncryptionError {
    fn from(error: AeadError) -> Self {
        match error {
            AeadError::Seal => Self::EncryptionFailed,
            AeadError::Open => Self::DecryptionFailed,
        }
    }
}

/// Load a 32-byte key from a base64-encoded string (e.g. `{PREFIX}_OAUTH_ENCRYPTION_KEY`).
///
/// Returns an error (not a panic) so callers can surface a clear operator message at
/// startup and refuse to proceed rather than silently using a bad key.
pub fn load_key(base64_str: &str) -> Result<EncryptionKey, EncryptionError> {
    let mut bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        base64_str.trim(),
    )
    .map_err(|e| EncryptionError::InvalidKey(format!("base64 decode error: {e}")))?;

    if bytes.len() != 32 {
        bytes.zeroize();
        return Err(EncryptionError::InvalidKey(format!(
            "expected 32 bytes, got {}",
            bytes.len()
        )));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    bytes.zeroize();
    Ok(EncryptionKey(key))
}

/// Encrypt `plaintext` under `key`, returning `(ciphertext, nonce)`.
///
/// A fresh random 12-byte nonce is generated internally on every call.
/// The caller MUST persist the returned nonce alongside the ciphertext.
#[allow(dead_code)]
pub fn seal(key: &EncryptionKey, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), EncryptionError> {
    seal_with_aad(key, plaintext, &[])
}

pub fn seal_with_aad(
    key: &EncryptionKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), EncryptionError> {
    let (ciphertext, nonce_bytes) = aead::seal(&key.0, plaintext, aad)?;
    Ok((ciphertext, nonce_bytes.to_vec()))
}

/// Decrypt `ciphertext` using `key` and `nonce`.
///
/// On failure (wrong key, wrong nonce, or tampered ciphertext) returns
/// `EncryptionError::DecryptionFailed`. Callers MUST surface this as
/// `oauth_needs_reauth`, not `internal_error`.
#[allow(dead_code)]
pub fn open(
    key: &EncryptionKey,
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    open_with_aad(key, ciphertext, nonce, &[])
}

pub fn open_with_aad(
    key: &EncryptionKey,
    ciphertext: &[u8],
    nonce: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    aead::open(&key.0, nonce, ciphertext, aad).map_err(EncryptionError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> EncryptionKey {
        load_key(&base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            [0u8; 32],
        ))
        .unwrap()
    }

    #[test]
    fn round_trip_plaintext() {
        let key = test_key();
        let plaintext = b"hello, world";
        let (ct, nonce) = seal(&key, plaintext).unwrap();
        let pt = open(&key, &ct, &nonce).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let key1 = test_key();
        let key2 = load_key(&base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            [1u8; 32],
        ))
        .unwrap();
        let (ct, nonce) = seal(&key1, b"secret").unwrap();
        assert!(open(&key2, &ct, &nonce).is_err());
    }

    #[test]
    fn wrong_nonce_fails_decryption() {
        let key = test_key();
        let (ct, _) = seal(&key, b"secret").unwrap();
        let bad_nonce = vec![0u8; 12];
        assert!(open(&key, &ct, &bad_nonce).is_err());
    }

    #[test]
    fn two_seals_produce_different_nonces() {
        let key = test_key();
        let (_, nonce1) = seal(&key, b"same plaintext").unwrap();
        let (_, nonce2) = seal(&key, b"same plaintext").unwrap();
        assert_ne!(nonce1, nonce2, "nonce reuse detected");
    }

    #[test]
    fn short_key_rejected() {
        let short = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 16]);
        assert!(load_key(&short).is_err());
    }

    #[test]
    fn invalid_base64_rejected() {
        assert!(load_key("not-valid-base64!!!").is_err());
    }

    #[test]
    fn key_debug_is_redacted() {
        let key = test_key();
        let debug = format!("{key:?}");
        assert_eq!(debug, "EncryptionKey(<redacted>)");
    }

    #[test]
    fn decrypt_failure_maps_to_decryption_failed() {
        let key = test_key();
        let (ct, nonce) = seal_with_aad(&key, b"secret", b"alice").unwrap();
        let err = open_with_aad(&key, &ct, &nonce, b"bob").unwrap_err();
        assert!(matches!(err, EncryptionError::DecryptionFailed));
    }

    #[test]
    fn aad_round_trip_plaintext() {
        let key = test_key();
        let aad = b"upstream=test\0subject=alice\0client=soma-client";
        let plaintext = b"hello, world";
        let (ct, nonce) = seal_with_aad(&key, plaintext, aad).unwrap();
        let pt = open_with_aad(&key, &ct, &nonce, aad).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn wrong_aad_fails_decryption() {
        let key = test_key();
        let (ct, nonce) = seal_with_aad(&key, b"secret", b"alice").unwrap();
        assert!(open_with_aad(&key, &ct, &nonce, b"bob").is_err());
    }
}
