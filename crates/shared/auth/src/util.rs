#![allow(clippy::redundant_pub_crate)]

use std::fmt::Write as _;
#[cfg(feature = "http-axum")]
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
#[cfg(feature = "http-axum")]
use std::time::Duration;

#[cfg(feature = "http-axum")]
use axum::Json;
#[cfg(feature = "http-axum")]
use axum::http::{HeaderValue, StatusCode, header};
#[cfg(feature = "http-axum")]
use axum::response::{IntoResponse, Response};
#[cfg(feature = "http-axum")]
use base64::Engine;
#[cfg(feature = "http-axum")]
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::error::AuthError;
#[cfg(feature = "http-axum")]
use crate::error::AuthErrorKind;

/// Extract the `IpAddr` from a `SocketAddr`, normalizing IPv4-mapped IPv6
/// addresses (`::ffff:a.b.c.d`) back to plain IPv4 so per-IP rate-limiting
/// keys are consistent regardless of listener address family.
#[cfg(feature = "http-axum")]
pub(crate) fn remote_ip(addr: SocketAddr) -> IpAddr {
    match addr.ip() {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}

pub fn now_unix() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i64::try_from(secs).unwrap_or(i64::MAX)
}

#[cfg(feature = "http-axum")]
pub(crate) fn random_token(bytes: usize) -> Result<String, AuthError> {
    let mut buf = vec![0_u8; bytes];
    getrandom::fill(&mut buf)
        .map_err(|error| AuthError::Storage(format!("generate random token: {error}")))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

pub fn fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut output = String::with_capacity(12);
    for byte in &digest[..6] {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(unix)]
pub(crate) fn ensure_restrictive_permissions(path: &Path) -> Result<(), AuthError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path)
        .map_err(|error| AuthError::Storage(format!("stat `{}`: {error}", path.display())))?;
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(AuthError::InsecurePermissions {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn ensure_restrictive_permissions(_path: &Path) -> Result<(), AuthError> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_restrictive_permissions(path: &Path) -> Result<(), AuthError> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| AuthError::Storage(format!("chmod 0600 `{}`: {error}", path.display())))
}

#[cfg(not(unix))]
pub(crate) fn set_restrictive_permissions(_path: &Path) -> Result<(), AuthError> {
    Ok(())
}

#[cfg(feature = "http-axum")]
pub(crate) fn duration_secs_i64(duration: Duration, field: &str) -> Result<i64, AuthError> {
    i64::try_from(duration.as_secs())
        .map_err(|_| AuthError::Config(format!("{field} exceeds supported range")))
}

#[cfg(feature = "http-axum")]
pub(crate) fn duration_secs_usize(duration: Duration, field: &str) -> Result<usize, AuthError> {
    usize::try_from(duration.as_secs())
        .map_err(|_| AuthError::Config(format!("{field} exceeds supported range")))
}

#[cfg(feature = "http-axum")]
pub(crate) fn timestamp_usize(timestamp: i64, field: &str) -> Result<usize, AuthError> {
    usize::try_from(timestamp)
        .map_err(|_| AuthError::Storage(format!("{field} is negative or exceeds usize range")))
}

/// Stamp `Cache-Control: no-store` on a response, without the HTTP/1.0-era
/// `Pragma` companion.
///
/// This is the weaker of the crate's two cache stamps and exists because
/// `authorize.rs`'s browser-facing HTML pages and the `/native/poll` JSON
/// only ever carried `Cache-Control`. Adding `Pragma` there would be a
/// behaviour change, so the difference is kept explicit in the name rather
/// than quietly harmonized. Anything that can carry token material or an
/// OAuth error object wants [`apply_no_store`] instead.
#[cfg(feature = "http-axum")]
pub(crate) fn apply_cache_control_no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Stamp the RFC 6749 section 5.1 `Cache-Control: no-store` +
/// `Pragma: no-cache` pair on a response.
///
/// Used by every surface that can return token material or an OAuth error
/// object: `/token` (success and failure), `/revoke`, `/register`, and
/// [`AuthError`]'s own `IntoResponse`.
#[cfg(feature = "http-axum")]
pub(crate) fn apply_no_store(response: Response) -> Response {
    let mut response = apply_cache_control_no_store(response);
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}

/// Build the RFC 6749 section 5.2 error object shared by `/token`
/// (RFC 6749), `/revoke` (RFC 7009 section 2.2.1, which adopts 5.2
/// wholesale), and `/register` (RFC 7591 section 3.2.2, which uses the same
/// two-field body with its own registry of `error` codes).
///
/// Each endpoint keeps its own variant-to-code, variant-to-status, and
/// variant-to-description mapping -- those genuinely differ, and RFC 7591's
/// codes are a different registry from RFC 6749's. Only the assembly is
/// shared: the JSON body, the [`AuthErrorKind`] response extension the access
/// log reads, the `Retry-After` seconds conversion, and the cache stamp.
#[cfg(feature = "http-axum")]
pub(crate) fn oauth_error_response(
    status: StatusCode,
    oauth_error: &'static str,
    description: String,
    log_kind: &'static str,
    retry_after_ms: Option<u64>,
) -> Response {
    let body = Json(serde_json::json!({
        "error": oauth_error,
        "error_description": description,
    }));
    let mut response = (status, body).into_response();
    response.extensions_mut().insert(AuthErrorKind(log_kind));
    if let Some(retry_after_ms) = retry_after_ms
        && let Ok(value) = HeaderValue::from_str(&(retry_after_ms / 1_000).max(1).to_string())
    {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    apply_no_store(response)
}

#[cfg(feature = "http-axum")]
pub(crate) fn expires_at(
    created_at: i64,
    duration: Duration,
    field: &str,
) -> Result<i64, AuthError> {
    let ttl = duration_secs_i64(duration, field)?;
    created_at
        .checked_add(ttl)
        .ok_or_else(|| AuthError::Config(format!("{field} exceeds supported range")))
}
