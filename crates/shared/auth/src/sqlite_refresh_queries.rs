//! Refresh-token presence queries used to decide upstream consent behavior.

use rusqlite::params;

use crate::error::AuthError;
use crate::util::now_unix;

use super::{SqliteStore, sqlite_error};

impl SqliteStore {
    /// Whether any unexpired refresh token has ever been issued, for
    /// any client. This is a single-tenant, admin-only gateway, so "someone
    /// already completed the Google consent screen once" is a reasonable
    /// proxy for "we don't need to force full re-consent again" without
    /// having to know which subject is about to authenticate.
    ///
    /// No longer called internally — `authorize()` now uses the
    /// provider-scoped [`Self::has_any_refresh_token_for_provider`] instead
    /// (this unscoped version incorrectly treats "some OTHER provider
    /// already has a refresh token on file" as a reason to skip forced
    /// consent on a user's very first login with a *different* provider).
    /// Retained as general-purpose public API for other consumers of this
    /// shared crate, not because removing the internal call site was an
    /// accident — do not assume this is dead code to delete.
    pub async fn has_any_refresh_token(&self) -> Result<bool, AuthError> {
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM refresh_tokens WHERE expires_at > ?1)",
                params![now],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count != 0)
            .map_err(sqlite_error)
        })
        .await
    }

    /// Same as [`Self::has_any_refresh_token`], scoped to one provider.
    ///
    /// `authorize()` (Task 11) uses this instead of the unscoped version to
    /// decide whether to force the upstream consent screen — the unscoped
    /// version incorrectly treats "Google already has a refresh token on
    /// file" as a reason to skip forced consent on a user's very first
    /// Authelia or GitHub login, silently degrading that new provider's
    /// first session to no local refresh token.
    pub async fn has_any_refresh_token_for_provider(
        &self,
        provider: &str,
    ) -> Result<bool, AuthError> {
        let provider = provider.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM refresh_tokens WHERE provider = ?1 AND expires_at > ?2)",
                params![provider, now],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count != 0)
            .map_err(sqlite_error)
        })
        .await
    }
}
