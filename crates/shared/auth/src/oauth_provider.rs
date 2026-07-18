use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::error::AuthError;

/// Parameters for building an upstream provider's `/authorize`-equivalent
/// redirect URL. `AuthorizeUrlRequest` was originally Google-specific
/// (`google::AuthorizeUrlRequest`); it moved here unchanged when the
/// `OAuthProvider` trait was introduced, since every provider needs the same
/// shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizeUrlRequest {
    pub state: String,
    pub scope: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    /// Force the upstream's full consent screen even if the user already
    /// granted these scopes. Needed the first time (to guarantee a refresh
    /// token comes back for providers that support one), but forcing it on
    /// every retry adds a slow, interactive round trip that impatient MCP
    /// clients can time out on before the human finishes clicking through it.
    ///
    /// This field is honestly OIDC/Google-shaped, not provider-neutral, and
    /// that's surfaced here rather than hidden: Google's `prompt=consent` is
    /// documented, verified behavior for guaranteeing a refresh token on
    /// re-authorization. Authelia's need for the same treatment is plausible
    /// (same `prompt` parameter, same OIDC family) but unverified against a
    /// real Authelia instance by this plan — treat it as inherited-but-not-
    /// proven. GitHub has no documented `prompt` parameter and no consent-
    /// gated refresh-token semantics at all (OAuth Apps never issue refresh
    /// tokens, full stop); `GitHubProvider::authorize_url` still appends
    /// `prompt=consent` when this is `true` purely because GitHub silently
    /// ignores unrecognized query params — it's dead weight, not a bug, but
    /// don't read "GitHub honors force_consent" into that.
    pub force_consent: bool,
}

/// Normalized result of a successful upstream code exchange or refresh,
/// common to every [`OAuthProvider`] implementation.
///
/// `id_token` is `Some` for OIDC-shaped providers (Google, Authelia) and
/// `None` for plain-OAuth2 providers with no ID token (GitHub).
///
/// `access_token`/`refresh_token`/`id_token` are `#[serde(skip_serializing)]`
/// as defense-in-depth: nothing in this plan serializes a whole
/// `ProviderExchange` to a client response or log line today (every call
/// site destructures individual non-secret fields), but nothing about the
/// type's shape should make that mistake easy for a future edit to make
/// silently — nothing in this crate's existing secret-handling discipline
/// (`fingerprint()`-before-log everywhere else) relies on "don't accidentally
/// serialize the whole struct" being enforced only by convention.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderExchange {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    #[serde(skip_serializing)]
    pub access_token: String,
    #[serde(skip_serializing)]
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    #[serde(skip_serializing)]
    pub id_token: Option<String>,
}

/// An upstream identity provider soma-auth can redirect a user to for login.
///
/// Implementations: [`crate::google::GoogleProvider`],
/// [`crate::authelia::AutheliaProvider`], [`crate::github::GitHubProvider`].
/// `AuthState.providers` holds a `provider_id() -> Arc<dyn OAuthProvider>`
/// map so a deployment can enable more than one simultaneously.
#[async_trait]
pub trait OAuthProvider: Send + Sync + std::fmt::Debug {
    /// Stable identifier used as the `providers` map key, the `provider`
    /// column value persisted in SQLite, and the subject-namespace prefix.
    /// One of `"google"`, `"authelia"`, `"github"`.
    fn provider_id(&self) -> &'static str;

    /// The absolute path (no scheme/host) this provider's registered
    /// `redirect_uri` resolves to, e.g. `/auth/google/callback`. Used by
    /// `routes::router` to mount one callback route per configured provider.
    fn callback_path(&self) -> &str;

    fn authorize_url(&self, request: &AuthorizeUrlRequest) -> Result<Url, AuthError>;

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderExchange, AuthError>;

    async fn refresh(&self, refresh_token: &str) -> Result<ProviderExchange, AuthError>;
}

/// Namespace a raw upstream subject by provider so two different IdPs
/// sharing one SQLite DB cannot collide on the same `subject` value.
///
/// Google is deliberately exempted (returns `raw_subject` unchanged): its
/// subject format predates multi-provider support, and already-issued
/// sessions/refresh tokens in production DBs have the bare, unprefixed
/// format. Changing it would silently invalidate every existing Google
/// session on upgrade. Authelia and GitHub are new — there is no existing
/// data to break, so they get the safer namespaced form from day one.
///
/// Only called from `authorize.rs`/`token.rs`, both gated behind
/// `http-axum` — a build of this crate with that feature off (and
/// `--all-targets` but not `--tests`) has no production caller, hence the
/// conditional `allow`. The two unit tests below exercise this function
/// unconditionally regardless of `http-axum`.
#[cfg_attr(not(feature = "http-axum"), allow(dead_code))]
pub(crate) fn namespaced_subject(provider_id: &str, raw_subject: &str) -> String {
    if provider_id == "google" {
        raw_subject.to_string()
    } else {
        format!("{provider_id}:{raw_subject}")
    }
}

#[cfg(test)]
mod tests {
    use super::namespaced_subject;

    #[test]
    fn google_subject_is_not_namespaced() {
        assert_eq!(namespaced_subject("google", "108123456"), "108123456");
    }

    #[test]
    fn non_google_subjects_are_namespaced() {
        assert_eq!(namespaced_subject("github", "9182310"), "github:9182310");
        assert_eq!(namespaced_subject("authelia", "alice"), "authelia:alice");
    }
}
