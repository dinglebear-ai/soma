//! Shared ChaCha20-Poly1305 AEAD core for at-rest encryption.
//!
//! Both at-rest stacks — inbound provider refresh tokens ([`crate::at_rest`])
//! and upstream OAuth credentials (`crate::upstream::encryption`) — call into
//! this module for the actual cipher operations, so the crate carries exactly
//! one seal/open implementation. Key parsing, storage formats, and error
//! taxonomies stay with the respective callers; this module only does the
//! AEAD math.
//!
//! Every [`seal`] call generates a **fresh random 12-byte nonce**. Callers
//! MUST persist the returned nonce alongside the ciphertext (inline in the
//! blob or in a separate column) and MUST NOT reuse it.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use getrandom::fill_uninit;
use std::mem::MaybeUninit;

/// ChaCha20-Poly1305 nonce length in bytes.
pub(crate) const NONCE_LEN: usize = 12;

/// Cipher-level failures. Deliberately detail-free so callers map them onto
/// their own error taxonomy without leaking cryptographic internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AeadError {
    /// Nonce generation or encryption failed.
    Seal,
    /// Wrong key, wrong nonce, mismatched AAD, or tampered ciphertext.
    Open,
}

/// Encrypt `plaintext` under `key`, binding `aad`, returning
/// `(ciphertext+tag, nonce)`.
///
/// Pass an empty `aad` slice for unbound (legacy-format) ciphertexts.
pub(crate) fn seal(
    key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<(Vec<u8>, [u8; NONCE_LEN]), AeadError> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|_| AeadError::Seal)?;

    let mut nonce_storage = [MaybeUninit::uninit(); NONCE_LEN];
    let nonce_bytes: [u8; NONCE_LEN] = fill_uninit(&mut nonce_storage)
        .map_err(|_| AeadError::Seal)?
        .try_into()
        .map_err(|_| AeadError::Seal)?;
    let nonce = Nonce::from(nonce_bytes);

    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AeadError::Seal)?;

    Ok((ciphertext, nonce_bytes))
}

/// Decrypt `ciphertext` using `key` and `nonce`, verifying `aad`.
///
/// Fails closed with [`AeadError::Open`] on wrong key, wrong nonce,
/// mismatched AAD, or tampered ciphertext — indistinguishably, by design.
pub(crate) fn open(
    key: &[u8; 32],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, AeadError> {
    if nonce.len() != NONCE_LEN {
        return Err(AeadError::Open);
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|_| AeadError::Open)?;
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AeadError::Open)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7u8; 32];

    #[test]
    fn seal_open_round_trip_with_aad() {
        let (ct, nonce) = seal(&KEY, b"payload", b"row-identity").unwrap();
        let pt = open(&KEY, &nonce, &ct, b"row-identity").unwrap();
        assert_eq!(pt, b"payload");
    }

    #[test]
    fn open_with_wrong_aad_fails() {
        let (ct, nonce) = seal(&KEY, b"payload", b"row-a").unwrap();
        assert_eq!(
            open(&KEY, &nonce, &ct, b"row-b").unwrap_err(),
            AeadError::Open
        );
    }

    #[test]
    fn open_with_wrong_nonce_length_fails() {
        let (ct, _) = seal(&KEY, b"payload", b"").unwrap();
        assert_eq!(
            open(&KEY, &[0u8; 11], &ct, b"").unwrap_err(),
            AeadError::Open
        );
    }
}
