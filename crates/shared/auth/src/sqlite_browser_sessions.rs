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

    /// Store a native-flow authorization code keyed by `state`, for the
    /// polling desktop client to retrieve via `take_native_authorization_result`.
    ///
    /// Last-write-wins on a `state` collision (e.g. a client retrying
    /// `/authorize` with the same `state` after a timeout): each row is
    /// single-use (deleted on first successful poll), so overwriting with the
    /// newest code is correct — silently dropping the newest code instead
    /// (`DO NOTHING`) would leave the polling client hung until the row's TTL
    /// expires, with no error surfaced anywhere.
    pub async fn insert_native_authorization_result(
        &self,
        result: NativeAuthorizationResultRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO native_authorization_results (state, code, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(state) DO UPDATE SET
                    code = excluded.code,
                    created_at = excluded.created_at,
                    expires_at = excluded.expires_at",
                params![
                    result.state,
                    result.code,
                    result.created_at,
                    result.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    /// One-shot read-and-delete of a pending native-flow authorization code.
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
                 RETURNING state, code, created_at, expires_at",
                params![state, now],
                row_to_native_authorization_result,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }
}
