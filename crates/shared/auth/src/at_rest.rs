/// At-rest encryption for upstream provider refresh tokens.
///
/// Provider tokens (e.g. Google refresh tokens) are encrypted with
/// ChaCha20-Poly1305 before being written to SQLite so that a copied
/// `auth.db` cannot be used as an upstream-account pivot.  The cipher
/// operations themselves live in the crate-internal `aead` core, shared with
/// the upstream OAuth credential store.
///
/// # Key management
///
/// The encryption key is a 32-byte value derived from the
/// `{PREFIX}_TOKEN_ENCRYPTION_KEY` environment variable, which must be
/// either 64 hex digits or 43 base64url-no-pad characters.  When the env
/// var is absent the helper functions are no-ops: data is stored as-is
/// (backward-compatible with existing deployments that haven't opted in to
/// at-rest protection yet).
///
/// # Storage formats
///
/// Two sentineled formats exist; both wrap `base64url(nonce || ciphertext+tag)`
/// where:
/// - `nonce` is a randomly generated 12-byte ChaCha20-Poly1305 nonce
/// - `ciphertext+tag` is the output of ChaCha20-Poly1305 AEAD encryption:
///   the ciphertext bytes followed by the 16-byte authentication tag appended
///   by the AEAD library
///
/// The `"enc2:"` prefix marks the current format: the ciphertext is sealed
/// with associated data (AAD) binding it to its row identity, so a blob
/// transplanted onto a different row fails authentication.  The legacy
/// `"enc:"` prefix marks ciphertexts sealed without AAD; they still decrypt
/// (back-compat) but are never written by new code.  Values with neither
/// prefix are legacy plaintext (or rows from databases without encryption)
/// and are returned as-is.
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::{self, AeadError, NONCE_LEN};
use crate::error::AuthError;

/// Legacy sentinel prefix: ciphertext sealed without AAD.
const ENC_PREFIX: &str = "enc:";

/// Current sentinel prefix: ciphertext sealed with row-identity AAD.
const ENC2_PREFIX: &str = "enc2:";

/// 32-byte ChaCha20-Poly1305 key.  Wiped from memory on drop.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct TokenEncryptionKey([u8; 32]);

impl std::fmt::Debug for TokenEncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenEncryptionKey(<redacted>)")
    }
}

impl TokenEncryptionKey {
    /// Parse a 32-byte key from a hex (64 chars) or base64url-no-pad (43 chars) string.
    pub fn from_encoded(s: &str) -> Result<Self, AuthError> {
        let s = s.trim();
        if s.len() == 64 {
            // 64-char hex string
            let mut key = [0u8; 32];
            for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
                let Ok(hex) = std::str::from_utf8(chunk) else {
                    key.zeroize();
                    return Err(AuthError::Config(
                        "TOKEN_ENCRYPTION_KEY hex string contains invalid UTF-8".to_string(),
                    ));
                };
                let Ok(byte) = u8::from_str_radix(hex, 16) else {
                    key.zeroize();
                    return Err(AuthError::Config(format!(
                        "TOKEN_ENCRYPTION_KEY hex string contains invalid character `{hex}`"
                    )));
                };
                key[i] = byte;
            }
            Ok(Self(key))
        } else {
            // Try base64url decode
            let mut bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| {
                AuthError::Config(
                    "TOKEN_ENCRYPTION_KEY must be 64 hex digits or 43 base64url characters"
                        .to_string(),
                )
            })?;
            if bytes.len() != 32 {
                bytes.zeroize();
                return Err(AuthError::Config(format!(
                    "TOKEN_ENCRYPTION_KEY base64-decoded to {} bytes, expected 32",
                    bytes.len()
                )));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            bytes.zeroize();
            Ok(Self(key))
        }
    }

    /// Derive a deterministic 32-byte key from an arbitrary passphrase using
    /// SHA-256.  Not for production use — provided so tests can create keys
    /// without managing hex strings.
    #[cfg(test)]
    pub fn from_passphrase(passphrase: &str) -> Self {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(passphrase.as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(&hash);
        Self(key)
    }
}

/// Seal `plaintext` with `aad` and encode as `<prefix><base64url(nonce||ct)>`.
fn seal_to_string(
    key: &TokenEncryptionKey,
    plaintext: &str,
    aad: &[u8],
    prefix: &str,
) -> Result<String, AuthError> {
    let (ciphertext, nonce_bytes) = aead::seal(&key.0, plaintext.as_bytes(), aad)
        .map_err(|_| AuthError::Storage("token encryption failed".to_string()))?;

    // Layout: nonce (12 bytes) || ciphertext+tag
    let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);

    Ok(format!("{prefix}{}", URL_SAFE_NO_PAD.encode(&blob)))
}

/// Decode a `base64url(nonce||ct)` payload and open it with `aad`.
fn open_from_encoded(
    key: &TokenEncryptionKey,
    encoded: &str,
    aad: &[u8],
) -> Result<String, AuthError> {
    let blob = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| {
        AuthError::Storage("provider token ciphertext is not valid base64".to_string())
    })?;

    if blob.len() < NONCE_LEN {
        return Err(AuthError::Storage(
            "provider token ciphertext blob too short".to_string(),
        ));
    }

    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let plaintext_bytes =
        aead::open(&key.0, nonce_bytes, ciphertext, aad).map_err(|_: AeadError| {
            AuthError::Storage(
                "provider token decryption failed (wrong key, wrong row binding, or corrupted data)"
                    .to_string(),
            )
        })?;

    String::from_utf8(plaintext_bytes)
        .map_err(|_| AuthError::Storage("decrypted provider token is not valid UTF-8".to_string()))
}

/// Encrypt a provider refresh token in the **legacy** unbound format.
///
/// Returns `"enc:<base64url(nonce||ciphertext)>"`.  New write paths should
/// use [`encrypt_provider_token_bound`] so the ciphertext is tied to its row
/// identity; this function remains for compatibility and tests.
pub fn encrypt_provider_token(
    key: &TokenEncryptionKey,
    plaintext: &str,
) -> Result<String, AuthError> {
    seal_to_string(key, plaintext, &[], ENC_PREFIX)
}

/// Encrypt a provider refresh token with AAD binding it to its row identity.
///
/// Returns `"enc2:<base64url(nonce||ciphertext)>"`.  The same `aad` bytes
/// must be supplied to [`decrypt_provider_token_bound`]; a ciphertext moved
/// to a row with a different identity fails decryption.
pub fn encrypt_provider_token_bound(
    key: &TokenEncryptionKey,
    plaintext: &str,
    aad: &[u8],
) -> Result<String, AuthError> {
    seal_to_string(key, plaintext, aad, ENC2_PREFIX)
}

/// Decrypt a provider refresh token that was encrypted by
/// [`encrypt_provider_token`].
///
/// Equivalent to [`decrypt_provider_token_bound`] with empty AAD: `"enc2:"`
/// values sealed with a non-empty binding fail closed here.
pub fn decrypt_provider_token(key: &TokenEncryptionKey, stored: &str) -> Result<String, AuthError> {
    decrypt_provider_token_bound(key, stored, &[])
}

/// Decrypt a stored provider refresh token, verifying `aad` for the current
/// `"enc2:"` format.
///
/// Legacy `"enc:"` values carry no binding and decrypt regardless of `aad`
/// (back-compat with rows written before AAD binding existed).  If the
/// stored value has neither prefix it is returned as-is — this handles
/// legacy plaintext rows and the case where no encryption key is configured.
pub fn decrypt_provider_token_bound(
    key: &TokenEncryptionKey,
    stored: &str,
    aad: &[u8],
) -> Result<String, AuthError> {
    if let Some(encoded) = stored.strip_prefix(ENC2_PREFIX) {
        open_from_encoded(key, encoded, aad)
    } else if let Some(encoded) = stored.strip_prefix(ENC_PREFIX) {
        // Legacy unbound ciphertext — no AAD to verify.
        open_from_encoded(key, encoded, &[])
    } else {
        // Not encrypted (legacy row or no-key path) — return plaintext.
        Ok(stored.to_string())
    }
}

/// Attempt to encrypt `value` in the legacy unbound format if a key is
/// available.  If `key` is `None` the value is stored as plaintext.  New
/// write paths should use [`maybe_encrypt_bound`].
pub fn maybe_encrypt(key: Option<&TokenEncryptionKey>, value: &str) -> Result<String, AuthError> {
    match key {
        Some(k) => encrypt_provider_token(k, value),
        None => Ok(value.to_string()),
    }
}

/// Attempt to encrypt `value` with row-identity AAD if a key is available,
/// returning the stored representation.  If `key` is `None` the value is
/// stored as plaintext.
pub fn maybe_encrypt_bound(
    key: Option<&TokenEncryptionKey>,
    value: &str,
    aad: &[u8],
) -> Result<String, AuthError> {
    match key {
        Some(k) => encrypt_provider_token_bound(k, value, aad),
        None => Ok(value.to_string()),
    }
}

/// Attempt to decrypt `stored` if it carries an encryption sentinel.
/// Equivalent to [`maybe_decrypt_bound`] with empty AAD.
pub fn maybe_decrypt(key: Option<&TokenEncryptionKey>, stored: &str) -> Result<String, AuthError> {
    maybe_decrypt_bound(key, stored, &[])
}

/// Attempt to decrypt `stored` if it carries the `"enc2:"` or `"enc:"`
/// prefix, verifying `aad` for the bound format.  If no key is provided but
/// the value is encrypted, return an error — the caller should not silently
/// return ciphertext as the token value.
pub fn maybe_decrypt_bound(
    key: Option<&TokenEncryptionKey>,
    stored: &str,
    aad: &[u8],
) -> Result<String, AuthError> {
    if stored.starts_with(ENC2_PREFIX) || stored.starts_with(ENC_PREFIX) {
        let k = key.ok_or_else(|| {
            AuthError::Config(
                "provider token is encrypted but no TOKEN_ENCRYPTION_KEY is configured".to_string(),
            )
        })?;
        decrypt_provider_token_bound(k, stored, aad)
    } else {
        // Plaintext — return as-is regardless of whether a key is present.
        Ok(stored.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> TokenEncryptionKey {
        TokenEncryptionKey::from_passphrase("test-key-for-unit-tests")
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = test_key();
        let token = "fake-google-refresh-token-for-tests";
        let encrypted = encrypt_provider_token(&key, token).unwrap();
        assert!(
            encrypted.starts_with(ENC_PREFIX),
            "ciphertext should carry enc: prefix"
        );
        assert_ne!(encrypted, token, "ciphertext must not equal plaintext");
        let decrypted = decrypt_provider_token(&key, &encrypted).unwrap();
        assert_eq!(decrypted, token);
    }

    #[test]
    fn bound_encrypt_decrypt_round_trip() {
        let key = test_key();
        let token = "fake-google-refresh-token-for-tests";
        let aad = b"refresh_token_hash=abc123";
        let encrypted = encrypt_provider_token_bound(&key, token, aad).unwrap();
        assert!(
            encrypted.starts_with(ENC2_PREFIX),
            "bound ciphertext should carry enc2: prefix"
        );
        assert_ne!(encrypted, token, "ciphertext must not equal plaintext");
        let decrypted = decrypt_provider_token_bound(&key, &encrypted, aad).unwrap();
        assert_eq!(decrypted, token);
    }

    #[test]
    fn bound_ciphertext_fails_with_wrong_aad() {
        let key = test_key();
        let encrypted =
            encrypt_provider_token_bound(&key, "secret", b"refresh_token_hash=row-a").unwrap();
        let err = decrypt_provider_token_bound(&key, &encrypted, b"refresh_token_hash=row-b")
            .unwrap_err();
        assert!(
            err.to_string().contains("decryption failed"),
            "transplanted ciphertext must fail closed, got: {err}"
        );
        assert_eq!(err.kind(), "internal_error");
    }

    #[test]
    fn bound_ciphertext_fails_via_unbound_decrypt() {
        let key = test_key();
        let encrypted =
            encrypt_provider_token_bound(&key, "secret", b"refresh_token_hash=row-a").unwrap();
        assert!(
            decrypt_provider_token(&key, &encrypted).is_err(),
            "enc2 ciphertext must not decrypt without its AAD"
        );
    }

    #[test]
    fn legacy_enc_rows_decrypt_regardless_of_aad() {
        let key = test_key();
        let token = "legacy-token-written-before-aad-binding";
        let encrypted = encrypt_provider_token(&key, token).unwrap();
        // Bound decrypt with a row identity still accepts legacy blobs.
        let decrypted =
            decrypt_provider_token_bound(&key, &encrypted, b"refresh_token_hash=whatever").unwrap();
        assert_eq!(decrypted, token);
    }

    #[test]
    fn different_encryptions_of_same_token_differ() {
        let key = test_key();
        let token = "test-provider-token-same-value";
        let enc1 = encrypt_provider_token_bound(&key, token, b"aad").unwrap();
        let enc2 = encrypt_provider_token_bound(&key, token, b"aad").unwrap();
        assert_ne!(
            enc1, enc2,
            "fresh nonce per encrypt must produce distinct ciphertexts"
        );
    }

    #[test]
    fn legacy_plaintext_returned_unchanged_via_decrypt() {
        let key = test_key();
        let plaintext = "legacy-plaintext-no-prefix";
        let result = decrypt_provider_token_bound(&key, plaintext, b"aad").unwrap();
        assert_eq!(result, plaintext);
    }

    #[test]
    fn maybe_encrypt_without_key_is_identity() {
        let value = "some-token";
        let result = maybe_encrypt_bound(None, value, b"aad").unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn maybe_encrypt_bound_with_key_writes_enc2() {
        let key = test_key();
        let stored = maybe_encrypt_bound(Some(&key), "some-token", b"aad").unwrap();
        assert!(
            stored.starts_with(ENC2_PREFIX),
            "new writes must use the AAD-bound sentinel, got: {stored}"
        );
    }

    #[test]
    fn maybe_decrypt_without_key_returns_plaintext() {
        let result = maybe_decrypt_bound(None, "plaintext-token", b"aad").unwrap();
        assert_eq!(result, "plaintext-token");
    }

    #[test]
    fn maybe_decrypt_without_key_fails_on_encrypted_value() {
        let key = test_key();
        for encrypted in [
            encrypt_provider_token(&key, "secret").unwrap(),
            encrypt_provider_token_bound(&key, "secret", b"aad").unwrap(),
        ] {
            let err = maybe_decrypt_bound(None, &encrypted, b"aad").unwrap_err();
            assert!(
                err.to_string().contains("TOKEN_ENCRYPTION_KEY"),
                "should tell operator to configure key, got: {err}"
            );
        }
    }

    #[test]
    fn key_from_str_accepts_hex() {
        let hex = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let key = TokenEncryptionKey::from_encoded(hex).unwrap();
        assert_eq!(key.0[0], 0x01);
        assert_eq!(key.0[31], 0x20);
    }

    #[test]
    fn key_from_str_rejects_short_hex() {
        let err = TokenEncryptionKey::from_encoded("aabbcc").unwrap_err();
        assert!(err.to_string().contains("TOKEN_ENCRYPTION_KEY"));
    }

    #[test]
    fn key_debug_is_redacted() {
        let key = test_key();
        let debug = format!("{key:?}");
        assert_eq!(debug, "TokenEncryptionKey(<redacted>)");
    }

    #[test]
    fn tampered_ciphertext_fails_decryption() {
        let key = test_key();
        let encrypted = encrypt_provider_token_bound(&key, "secret", b"aad").unwrap();
        // Decode the base64url payload, flip a byte in the binary blob, re-encode.
        let encoded = encrypted.strip_prefix(ENC2_PREFIX).unwrap();
        let mut blob = URL_SAFE_NO_PAD.decode(encoded).unwrap();
        if blob.len() > 20 {
            blob[20] ^= 0xff;
        }
        let tampered = format!("{ENC2_PREFIX}{}", URL_SAFE_NO_PAD.encode(&blob));
        let result = decrypt_provider_token_bound(&key, &tampered, b"aad");
        assert!(
            result.is_err(),
            "tampered ciphertext should fail to decrypt"
        );
    }
}
