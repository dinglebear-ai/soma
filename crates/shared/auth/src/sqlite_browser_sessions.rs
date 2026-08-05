//! Persistence for the interactive login-session tables: `browser_sessions`,
//! `browser_login_states`, and `native_authorization_results`. Split out of
//! `sqlite.rs` to stay under the PATTERNS.md module size hard limit.
use rusqlite::{OptionalExtension, params};

use crate::error::AuthError;
use crate::types::{BrowserLoginStateRow, BrowserSessionRow, NativeAuthorizationResultRow};
use crate::util::now_unix;

use super::sqlite_rows::{
    row_to_browser_login_state, row_to_browser_session, row_to_native_authorization_result,
};
use super::{SqliteStore, sqlite_error};

impl SqliteStore {
    pub async fn upsert_browser_session(
        &self,
        session: BrowserSessionRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO browser_sessions (
                    session_id, subject, email, csrf_token, created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(session_id) DO UPDATE SET
                    subject = excluded.subject,
                    email = excluded.email,
                    csrf_token = excluded.csrf_token,
                    created_at = excluded.created_at,
                    expires_at = excluded.expires_at",
                params![
                    session.session_id,
                    session.subject,
                    session.email,
                    session.csrf_token,
                    session.created_at,
                    session.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_browser_session(
        &self,
        session_id: &str,
    ) -> Result<Option<BrowserSessionRow>, AuthError> {
        let session_id = session_id.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT session_id, subject, email, csrf_token, created_at, expires_at
                 FROM browser_sessions
                 WHERE session_id = ?1
                   AND expires_at > ?2",
                params![session_id, now],
                row_to_browser_session,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn revoke_browser_session(&self, session_id: &str) -> Result<(), AuthError> {
        let session_id = session_id.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM browser_sessions WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn insert_browser_login_state(
        &self,
        login: BrowserLoginStateRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO browser_login_states (
                    state, return_to, provider_code_verifier, created_at, expires_at, provider
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    login.state,
                    login.return_to,
                    login.provider_code_verifier,
                    login.created_at,
                    login.expires_at,
                    login.provider,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn take_browser_login_state(
        &self,
        state: &str,
    ) -> Result<Option<BrowserLoginStateRow>, AuthError> {
        let state = state.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM browser_login_states
                 WHERE state = ?1
                   AND expires_at > ?2
                 RETURNING state, return_to, provider_code_verifier, created_at, expires_at, provider",
                params![state, now],
                row_to_browser_login_state,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    /// Store a terminal native-flow result keyed by `state`, for the polling
    /// desktop client to retrieve via `take_native_authorization_result`.
    /// Exactly one of `code` or `error` must be present.
    ///
    /// An unexpired result is never overwritten. The native client state is a
    /// one-time poll credential, so replacing a live row would let a repeated
    /// authorization request invalidate or swap the result another poller is
    /// about to redeem. Expired rows may be replaced so retries recover.
    pub async fn insert_native_authorization_result(
        &self,
        result: NativeAuthorizationResultRow,
    ) -> Result<(), AuthError> {
        let (code, error) = match (result.code, result.error) {
            (Some(code), None) if !code.is_empty() => (code, None),
            (None, Some(error)) if !error.is_empty() => (String::new(), Some(error)),
            _ => {
                return Err(AuthError::Validation(
                    "native authorization result must contain exactly one non-empty code or error"
                        .to_string(),
                ));
            }
        };
        self.with_conn(move |conn| {
            let changed = conn
                .execute(
                    "INSERT INTO native_authorization_results
                        (state, code, error, created_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(state) DO UPDATE SET
                        code = excluded.code,
                        error = excluded.error,
                        created_at = excluded.created_at,
                        expires_at = excluded.expires_at
                     WHERE native_authorization_results.expires_at <= excluded.created_at",
                    params![
                        result.state,
                        code,
                        error,
                        result.created_at,
                        result.expires_at,
                    ],
                )
                .map_err(sqlite_error)?;
            if changed == 0 {
                return Err(AuthError::InvalidGrant(
                    "native authorization state already has a pending result".to_string(),
                ));
            }
            Ok(())
        })
        .await
    }

    /// One-shot read-and-delete of a pending native-flow terminal result.
    pub async fn take_native_authorization_result(
        &self,
        state: &str,
    ) -> Result<Option<NativeAuthorizationResultRow>, AuthError> {
        let state = state.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM native_authorization_results
                 WHERE state = ?1
                   AND expires_at > ?2
                 RETURNING state, code, error, created_at, expires_at",
                params![state, now],
                row_to_native_authorization_result,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }
}
