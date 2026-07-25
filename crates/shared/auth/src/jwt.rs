use std::fmt;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::SigningKey;
use ed25519_dalek::VerifyingKey;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AuthError;
use crate::util::{ensure_restrictive_permissions, now_unix, set_restrictive_permissions};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub exp: usize,
    pub iat: usize,
    pub jti: String,
    pub scope: String,
    pub azp: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwksDocument {
    pub keys: Vec<JwkKey>,
}

/// A single OKP/Ed25519 verification key advertised at `/jwks`.
///
/// Note the field shape is Ed25519's (`kty=OKP`, `crv=Ed25519`, raw public
/// point in `x`), not RSA's `n`/`e` — the local IdP migrated from RS256 to
/// EdDSA. Upstream provider JWKS (Google/Authelia) still use the separate
/// RSA-shaped `Jwk` type in the crate-internal `oidc` module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwkKey {
    pub kty: String,
    #[serde(rename = "use")]
    pub use_: String,
    pub alg: String,
    pub kid: String,
    pub crv: String,
    pub x: String,
}

/// One active verification key (kid + decoder), plus the JWK it publishes.
#[derive(Clone)]
struct Verifier {
    kid: String,
    decoding_key: DecodingKey,
    jwk: JwkKey,
}

/// Local access-token signing keys.
///
/// Signs with a single active Ed25519 key (`key_id` / `encoding_key`) but
/// verifies against every key in `verifiers` — the active key plus, after a
/// [`Self::rotate`], the immediately previous key. Publishing both in the
/// JWKS and accepting tokens signed by either gives an overlap window so a
/// key rotation does not instantly invalidate outstanding access tokens;
/// the previous key ages out on the next rotation (its still-valid tokens
/// bounded by the access-token TTL).
#[derive(Clone)]
pub struct SigningKeys {
    pub key_id: String,
    encoding_key: EncodingKey,
    verifiers: Vec<Verifier>,
    jwks: JwksDocument,
}

impl fmt::Debug for SigningKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKeys")
            .field("key_id", &self.key_id)
            .field("verifier_count", &self.verifiers.len())
            .finish_non_exhaustive()
    }
}

impl SigningKeys {
    /// Load the active signing key from `path` (generating one on first
    /// run), plus the previous key from the sibling `<path>.prev` slot if a
    /// prior [`Self::rotate`] left one there.
    ///
    /// Any key material that does not parse as an Ed25519 PKCS#8 DER key —
    /// notably a pre-migration RSA PEM key written by an older release — is
    /// quarantined to `<path>.retired-<unix-ts>` rather than reused, and a
    /// fresh Ed25519 key is generated in its place. The retired key is never
    /// promoted to a verifier: it is being retired precisely because its
    /// signing path is not the one we trust.
    pub fn load_or_create(path: &Path) -> Result<Self, AuthError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AuthError::Storage(format!(
                    "create signing key directory `{}`: {error}",
                    parent.display()
                ))
            })?;
        }

        let active = load_or_generate_key(path)?;
        let previous = load_optional_previous_key(&previous_key_path(path))?;
        Self::from_keys(&active, previous.as_ref())
    }

    /// Rotate the signing key while keeping the outgoing key valid for one
    /// overlap window.
    ///
    /// Moves the current active key into the `<path>.prev` slot (replacing
    /// any older previous key) and generates a fresh active key at `path`.
    /// Access tokens signed by the now-previous key keep validating until
    /// their TTL expires, because the returned [`SigningKeys`] still carries
    /// the previous key as a verifier and advertises it in the JWKS. The
    /// caller is responsible for swapping the new value in (e.g. replacing
    /// the `Arc<SigningKeys>` in `AuthState`).
    pub fn rotate(path: &Path) -> Result<Self, AuthError> {
        let outgoing = load_or_generate_key(path)?;
        let prev_path = previous_key_path(path);
        // Move active -> prev (fs::rename replaces any existing prev
        // atomically on Unix), then harden the relocated file.
        std::fs::rename(path, &prev_path).map_err(|error| {
            AuthError::Storage(format!(
                "rotate signing key `{}` -> `{}`: {error}",
                path.display(),
                prev_path.display()
            ))
        })?;
        set_restrictive_permissions(&prev_path)?;
        let active = generate_signing_key(path)?;
        // The just-moved key is the overlap verifier; drop the copy we read
        // before the move in favor of the on-disk one we now know is at
        // `prev_path` (identical bytes, but keeps the invariant that every
        // verifier corresponds to a file).
        drop(outgoing);
        let previous = load_optional_previous_key(&prev_path)?;
        Self::from_keys(&active, previous.as_ref())
    }

    pub fn issue_access_token(&self, claims: &AccessClaims) -> Result<String, AuthError> {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.key_id.clone());
        encode(&header, &claims, &self.encoding_key)
            .map_err(|error| AuthError::Storage(format!("encode access token: {error}")))
    }

    /// Validate access token signature, algorithm, and audience.
    ///
    /// NOTE: this method does NOT enforce the `iss` claim. Callers that
    /// need RFC 7519 issuer validation MUST use
    /// [`Self::validate_access_token_with_issuer`] instead. This entry
    /// point is preserved for the lab consumer, which performs its own
    /// post-decode `iss` check. New consumers should always use the
    /// issuer-enforcing variant.
    #[deprecated(note = "Use `validate_access_token_with_issuer` for RFC 7519 §4.1.1 compliance")]
    pub fn validate_access_token(
        &self,
        token: &str,
        expected_audience: &str,
    ) -> Result<AccessClaims, AuthError> {
        let decoding_key = self.decoding_key_for_token(token)?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_audience(&[expected_audience]);
        decode::<AccessClaims>(token, decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| AuthError::InvalidAccessToken)
    }

    /// Validate signature, algorithm, audience, AND issuer in a single
    /// pass — the issuer is enforced via `Validation::set_issuer` BEFORE
    /// decode (RFC 7519 §4.1.1 compliant) rather than via a manual
    /// `claims.iss != expected` check after decode.
    ///
    /// The verification key is selected by the token's `kid` header, so a
    /// token signed by the previous key during a rotation overlap still
    /// validates.
    pub fn validate_access_token_with_issuer(
        &self,
        token: &str,
        expected_audience: &str,
        expected_issuer: &str,
    ) -> Result<AccessClaims, AuthError> {
        let decoding_key = self.decoding_key_for_token(token)?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_audience(&[expected_audience]);
        validation.set_issuer(&[expected_issuer]);
        decode::<AccessClaims>(token, decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| AuthError::InvalidAccessToken)
    }

    pub const fn jwks(&self) -> &JwksDocument {
        &self.jwks
    }

    /// Select the verification key matching the token's `kid` header. A
    /// token whose `kid` names no known key is rejected up front (rather
    /// than trial-decoding against every key); a token with no `kid` falls
    /// back to the active key.
    fn decoding_key_for_token(&self, token: &str) -> Result<&DecodingKey, AuthError> {
        let header = decode_header(token).map_err(|_| AuthError::InvalidAccessToken)?;
        match header.kid {
            Some(kid) => self
                .verifiers
                .iter()
                .find(|verifier| verifier.kid == kid)
                .map(|verifier| &verifier.decoding_key)
                .ok_or(AuthError::InvalidAccessToken),
            None => self
                .verifiers
                .first()
                .map(|verifier| &verifier.decoding_key)
                .ok_or(AuthError::InvalidAccessToken),
        }
    }

    fn from_keys(active: &SigningKey, previous: Option<&SigningKey>) -> Result<Self, AuthError> {
        let active_der = active
            .to_pkcs8_der()
            .map_err(|error| AuthError::Storage(format!("encode signing key DER: {error}")))?;
        let active_verifier = build_verifier(&active.verifying_key())?;
        let key_id = active_verifier.kid.clone();

        let mut verifiers = vec![active_verifier];
        if let Some(previous) = previous {
            let previous_verifier = build_verifier(&previous.verifying_key())?;
            // Guard against a degenerate rotation that left an identical key
            // in the prev slot — publishing the same kid twice is harmless
            // but pointless.
            if previous_verifier.kid != key_id {
                verifiers.push(previous_verifier);
            }
        }

        let jwks = JwksDocument {
            keys: verifiers
                .iter()
                .map(|verifier| verifier.jwk.clone())
                .collect(),
        };

        Ok(Self {
            key_id,
            encoding_key: EncodingKey::from_ed_der(active_der.as_bytes()),
            verifiers,
            jwks,
        })
    }
}

/// Build the kid, decoder, and published JWK for one Ed25519 public key.
fn build_verifier(public_key: &VerifyingKey) -> Result<Verifier, AuthError> {
    let public_der = public_key
        .to_public_key_der()
        .map_err(|error| AuthError::Storage(format!("encode public key DER: {error}")))?;
    let digest = Sha256::digest(public_der.as_bytes());
    let kid = URL_SAFE_NO_PAD.encode(&digest[..12]);

    let jwk = JwkKey {
        kty: "OKP".to_string(),
        use_: "sig".to_string(),
        alg: "EdDSA".to_string(),
        kid: kid.clone(),
        crv: "Ed25519".to_string(),
        x: URL_SAFE_NO_PAD.encode(public_key.as_bytes()),
    };

    Ok(Verifier {
        kid,
        // jsonwebtoken's RustCrypto verifier consumes the raw 32-byte
        // Ed25519 public point here (its `from_ed_der` name is historical).
        decoding_key: DecodingKey::from_ed_der(public_key.as_bytes()),
        jwk,
    })
}

fn previous_key_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".prev");
    path.with_file_name(name)
}

/// Load an Ed25519 key from `path`, generating one if the file is missing
/// and quarantining any non-Ed25519 material (legacy RSA) before generating
/// a replacement.
fn load_or_generate_key(path: &Path) -> Result<SigningKey, AuthError> {
    if !path.exists() {
        return generate_signing_key(path);
    }

    ensure_restrictive_permissions(path)?;
    let key_bytes = std::fs::read(path).map_err(|error| {
        AuthError::Storage(format!("read signing key `{}`: {error}", path.display()))
    })?;
    match SigningKey::from_pkcs8_der(&key_bytes) {
        Ok(key) => {
            ensure_restrictive_permissions(path)?;
            Ok(key)
        }
        Err(_) => {
            quarantine_key(path)?;
            generate_signing_key(path)
        }
    }
}

/// Load the optional previous-key overlap slot. A present-but-unparseable
/// prev file (e.g. leftover RSA) is quarantined and treated as absent rather
/// than failing the whole load.
fn load_optional_previous_key(path: &Path) -> Result<Option<SigningKey>, AuthError> {
    if !path.exists() {
        return Ok(None);
    }
    ensure_restrictive_permissions(path)?;
    let key_bytes = std::fs::read(path).map_err(|error| {
        AuthError::Storage(format!(
            "read previous signing key `{}`: {error}",
            path.display()
        ))
    })?;
    match SigningKey::from_pkcs8_der(&key_bytes) {
        Ok(key) => Ok(Some(key)),
        Err(_) => {
            quarantine_key(path)?;
            Ok(None)
        }
    }
}

/// Rename a key file out of the way to `<path>.retired-<unix-ts>`, keeping
/// restrictive permissions, so it is neither reused nor deleted.
fn quarantine_key(path: &Path) -> Result<(), AuthError> {
    let retired = path.with_extension(format!("retired-{}", now_unix()));
    std::fs::rename(path, &retired).map_err(|error| {
        AuthError::Storage(format!(
            "retire legacy signing key `{}`: {error}",
            path.display()
        ))
    })?;
    set_restrictive_permissions(&retired)?;
    Ok(())
}

fn generate_signing_key(path: &Path) -> Result<SigningKey, AuthError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| AuthError::Storage(format!("generate Ed25519 key material: {error}")))?;
    let key = SigningKey::from_bytes(&bytes);
    bytes.fill(0);
    let der = key
        .to_pkcs8_der()
        .map_err(|error| AuthError::Storage(format!("encode signing key DER: {error}")))?;
    std::fs::write(path, der.as_bytes()).map_err(|error| {
        AuthError::Storage(format!("write signing key `{}`: {error}", path.display()))
    })?;
    set_restrictive_permissions(path)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{Algorithm, decode_header};

    use super::{AccessClaims, SigningKeys, previous_key_path};

    #[test]
    fn generated_key_is_reused_on_second_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-jwt.key");
        let first = SigningKeys::load_or_create(&path).unwrap();
        let second = SigningKeys::load_or_create(&path).unwrap();
        assert_eq!(first.key_id, second.key_id);
    }

    #[cfg(unix)]
    #[test]
    fn signing_key_refuses_world_readable_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-jwt.key");
        std::fs::write(&path, "bad").unwrap();
        std::fs::set_permissions(&path, PermissionsExt::from_mode(0o644)).unwrap();
        let err = SigningKeys::load_or_create(&path).unwrap_err();
        assert!(err.to_string().contains("permissions"));
    }

    #[test]
    #[allow(deprecated)]
    fn minted_access_token_round_trips_and_contains_kid() {
        let signer = test_signer();
        let claims = sample_claims();
        let token = signer.issue_access_token(&claims).unwrap();
        let claims = signer
            .validate_access_token(&token, "https://lab.example.com")
            .unwrap();
        assert_eq!(claims.aud, "https://lab.example.com");
        assert!(!claims.jti.is_empty());
        assert!(decode_header(&token).unwrap().kid.is_some());
    }

    #[test]
    fn minted_token_uses_eddsa_algorithm() {
        let signer = test_signer();
        let token = signer.issue_access_token(&sample_claims()).unwrap();
        assert_eq!(decode_header(&token).unwrap().alg, Algorithm::EdDSA);
    }

    #[test]
    fn jwks_advertises_okp_ed25519_key() {
        let signer = test_signer();
        let jwk = &signer.jwks().keys[0];
        assert_eq!(jwk.kty, "OKP");
        assert_eq!(jwk.crv, "Ed25519");
        assert_eq!(jwk.alg, "EdDSA");
        assert_eq!(jwk.kid, signer.key_id);
        assert!(!jwk.x.is_empty());
    }

    #[test]
    #[allow(deprecated)]
    fn wrong_audience_is_rejected() {
        let signer = test_signer();
        let claims = sample_claims();
        let token = signer.issue_access_token(&claims).unwrap();
        let result = signer.validate_access_token(&token, "https://other.example.com");
        assert!(
            result.is_err(),
            "token with wrong audience must be rejected"
        );
    }

    #[test]
    fn validate_with_issuer_accepts_matching_issuer() {
        let signer = test_signer();
        let claims = sample_claims();
        let token = signer.issue_access_token(&claims).unwrap();
        let decoded = signer
            .validate_access_token_with_issuer(
                &token,
                "https://lab.example.com",
                "https://lab.example.com",
            )
            .expect("token with matching issuer must validate");
        assert_eq!(decoded.iss, "https://lab.example.com");
    }

    #[test]
    fn validate_with_issuer_rejects_wrong_issuer_via_validation_struct() {
        // Locked decision: issuer enforcement uses Validation::set_issuer
        // BEFORE decode (so jsonwebtoken rejects up-front), not a manual
        // post-decode `claims.iss != expected` comparison.
        let signer = test_signer();
        let claims = sample_claims();
        let token = signer.issue_access_token(&claims).unwrap();
        let result = signer.validate_access_token_with_issuer(
            &token,
            "https://lab.example.com",
            "https://attacker.example.com",
        );
        assert!(
            result.is_err(),
            "token signed by us but with wrong expected issuer must be rejected"
        );
    }

    #[test]
    fn token_from_a_foreign_key_is_rejected() {
        let signer = test_signer();
        let stranger = test_signer();
        let token = stranger.issue_access_token(&sample_claims()).unwrap();
        let result = signer.validate_access_token_with_issuer(
            &token,
            "https://lab.example.com",
            "https://lab.example.com",
        );
        assert!(
            result.is_err(),
            "token signed by a different key (unknown kid) must be rejected"
        );
    }

    #[cfg(unix)]
    #[test]
    fn legacy_rsa_key_is_quarantined_and_replaced() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-jwt.key");
        // A pre-migration RSA PKCS#8 PEM file: valid old content, but not an
        // Ed25519 PKCS#8 DER key, so it must be quarantined rather than used.
        std::fs::write(
            &path,
            "-----BEGIN PRIVATE KEY-----\nMIIlegacyrsakeymaterial==\n-----END PRIVATE KEY-----\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, PermissionsExt::from_mode(0o600)).unwrap();

        let signer = SigningKeys::load_or_create(&path).unwrap();

        // A retired sibling captured the old key, and the new active key is a
        // working Ed25519 key.
        let retired: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".retired-"))
            .collect();
        assert_eq!(retired.len(), 1, "legacy key should be quarantined once");
        let token = signer.issue_access_token(&sample_claims()).unwrap();
        assert_eq!(decode_header(&token).unwrap().alg, Algorithm::EdDSA);
    }

    #[test]
    fn rotate_keeps_previous_key_valid_during_overlap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-jwt.key");
        let before = SigningKeys::load_or_create(&path).unwrap();
        let old_token = before.issue_access_token(&sample_claims()).unwrap();

        let after = SigningKeys::rotate(&path).unwrap();

        // New active key differs, and the JWKS now advertises both keys.
        assert_ne!(after.key_id, before.key_id);
        assert_eq!(after.jwks().keys.len(), 2);
        assert!(previous_key_path(&path).exists());

        // A token minted by the pre-rotation key still validates against the
        // rotated key set (overlap window)...
        after
            .validate_access_token_with_issuer(
                &old_token,
                "https://lab.example.com",
                "https://lab.example.com",
            )
            .expect("pre-rotation token must remain valid during overlap");

        // ...and freshly minted tokens validate too.
        let new_token = after.issue_access_token(&sample_claims()).unwrap();
        after
            .validate_access_token_with_issuer(
                &new_token,
                "https://lab.example.com",
                "https://lab.example.com",
            )
            .expect("post-rotation token must validate");
    }

    #[test]
    fn reload_after_rotate_still_carries_both_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-jwt.key");
        let _before = SigningKeys::load_or_create(&path).unwrap();
        let rotated = SigningKeys::rotate(&path).unwrap();
        let reloaded = SigningKeys::load_or_create(&path).unwrap();
        assert_eq!(reloaded.key_id, rotated.key_id);
        assert_eq!(
            reloaded.jwks().keys.len(),
            2,
            "a restart after rotation must reload the overlap key"
        );
    }

    fn test_signer() -> SigningKeys {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-jwt.key");
        SigningKeys::load_or_create(&path).unwrap()
    }

    fn sample_claims() -> AccessClaims {
        AccessClaims {
            iss: "https://lab.example.com".to_string(),
            sub: "google-user".to_string(),
            aud: "https://lab.example.com".to_string(),
            exp: 4_102_444_800,
            iat: 1_700_000_000,
            jti: "test-jti".to_string(),
            scope: "lab".to_string(),
            azp: "client".to_string(),
        }
    }
}
