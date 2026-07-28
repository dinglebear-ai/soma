use rusqlite::params;

use crate::error::AuthError;

use super::{SqliteStore, hash_token, sqlite_error};

const MAX_ASSERTION_ISSUER_BYTES: usize = 2048;
const MAX_ASSERTION_JTI_BYTES: usize = 256;
const MAX_ASSERTION_LIFETIME_SECS: i64 = 300;
const MAX_ASSERTION_CLOCK_SKEW_SECS: i64 = 60;

impl SqliteStore {
    /// Consume one JWT assertion identifier exactly once.
    ///
    /// The check and insert are atomic across processes sharing this database.
    pub async fn consume_assertion_jti(
        &self,
        issuer: &str,
        jti: &str,
        issued_at: i64,
        expires_at: i64,
        now: i64,
    ) -> Result<bool, AuthError> {
        if issuer.is_empty() || issuer.len() > MAX_ASSERTION_ISSUER_BYTES {
            return Ok(false);
        }
        if jti.is_empty() || jti.len() > MAX_ASSERTION_JTI_BYTES {
            return Ok(false);
        }
        if issued_at > now + MAX_ASSERTION_CLOCK_SKEW_SECS
            || expires_at <= now
            || expires_at.saturating_sub(issued_at) > MAX_ASSERTION_LIFETIME_SECS
        {
            return Ok(false);
        }

        let issuer = issuer.to_owned();
        let jti = jti.to_owned();
        self.with_conn(move |conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(sqlite_error)?;
            let result = (|| {
                conn.execute(
                    "DELETE FROM assertion_jtis WHERE expires_at <= ?1",
                    params![now],
                )
                .map_err(sqlite_error)?;
                conn.execute(
                    "INSERT OR IGNORE INTO assertion_jtis (issuer, jti, expires_at)
                     VALUES (?1, ?2, ?3)",
                    params![issuer, jti, expires_at],
                )
                .map(|inserted| inserted == 1)
                .map_err(sqlite_error)
            })();
            match result {
                Ok(consumed) => {
                    conn.execute_batch("COMMIT").map_err(sqlite_error)?;
                    Ok(consumed)
                }
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(error)
                }
            }
        })
        .await
    }

    /// Revoke one refresh token only when it belongs to the authenticated client.
    pub async fn revoke_refresh_token(
        &self,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<bool, AuthError> {
        let token_hash = hash_token(refresh_token);
        let client_id = client_id.to_owned();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM refresh_tokens
                 WHERE refresh_token_hash = ?1 AND client_id = ?2",
                params![token_hash, client_id],
            )
            .map(|deleted| deleted == 1)
            .map_err(sqlite_error)
        })
        .await
    }
}

#[cfg(test)]
#[path = "sqlite_assertions_tests.rs"]
mod tests;
