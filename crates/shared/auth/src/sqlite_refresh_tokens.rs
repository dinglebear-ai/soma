//! Refresh-token persistence, rotation, replay detection, and grant auth lookup.

use rusqlite::{OptionalExtension, params};

use crate::at_rest::{maybe_decrypt_bound, maybe_encrypt_bound};
use crate::error::AuthError;
use crate::types::RefreshTokenRow;
use crate::util::now_unix;

use super::{SqliteStore, hash_token, refresh_token_aad, sqlite_error};

impl SqliteStore {
    /// Insert a new refresh token row, storing a SHA-256 hash of the raw token
    /// as the primary key.  The plaintext token is **never** persisted; only the
    /// caller-returned value contains it.  If an encryption key is configured,
    /// `provider_refresh_token` is encrypted at rest before storage.
    ///
    /// Use [`Self::rotate_refresh_token`] instead of calling this twice when replacing
    /// an existing token — that method performs the swap atomically.
    pub async fn upsert_refresh_token(&self, token: RefreshTokenRow) -> Result<(), AuthError> {
        let hash = hash_token(&token.refresh_token);
        let encrypted_provider_rt = token
            .provider_refresh_token
            .as_deref()
            .map(|raw| maybe_encrypt_bound(self.enc_key.as_deref(), raw, &refresh_token_aad(&hash)))
            .transpose()?;
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO refresh_tokens (
                    refresh_token_hash, family_id, client_id, subject, resource, scope,
                    provider_refresh_token, created_at, expires_at, provider,
                    token_endpoint_auth_method
                 ) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(refresh_token_hash) DO UPDATE SET
                    client_id = excluded.client_id,
                    subject = excluded.subject,
                    resource = excluded.resource,
                    scope = excluded.scope,
                    provider_refresh_token = excluded.provider_refresh_token,
                    created_at = excluded.created_at,
                    expires_at = excluded.expires_at,
                    provider = excluded.provider,
                    token_endpoint_auth_method = excluded.token_endpoint_auth_method",
                params![
                    hash,
                    token.client_id,
                    token.subject,
                    token.resource,
                    token.scope,
                    encrypted_provider_rt,
                    token.created_at,
                    token.expires_at,
                    token.provider,
                    token.token_endpoint_auth_method,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    /// Atomically rotate a refresh token and retain its spent hash for replay
    /// detection. Every token in a grant shares a family id. Reusing any spent
    /// family member revokes the currently active member before returning
    /// `invalid_grant`.
    pub async fn rotate_refresh_token(
        &self,
        old_token: &str,
        new_token: RefreshTokenRow,
    ) -> Result<Option<RefreshTokenRow>, AuthError> {
        let old_hash = hash_token(old_token);
        let new_hash = hash_token(&new_token.refresh_token);
        let now = now_unix();
        let replay_client_id = new_token.client_id.clone();
        let replay_auth_method = new_token.token_endpoint_auth_method.clone();
        let family_expires_at = new_token.expires_at;
        let encrypted_provider_rt = new_token
            .provider_refresh_token
            .as_deref()
            .map(|raw| {
                maybe_encrypt_bound(self.enc_key.as_deref(), raw, &refresh_token_aad(&new_hash))
            })
            .transpose()?;
        self.with_conn(move |conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(sqlite_error)?;

            let family_id = conn
                .query_row(
                    "SELECT family_id FROM refresh_tokens
                     WHERE refresh_token_hash = ?1
                       AND client_id = ?2
                       AND expires_at > ?3",
                    params![old_hash, replay_client_id, now],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(sqlite_error)?;

            let Some(family_id) = family_id else {
                let replay_family = conn
                    .query_row(
                        "SELECT family_id FROM used_refresh_tokens
                         WHERE refresh_token_hash = ?1
                           AND client_id = ?2
                           AND expires_at > ?3",
                        params![old_hash, replay_client_id, now],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sqlite_error)?;
                if let Some(replay_family) = replay_family {
                    conn.execute(
                        "DELETE FROM refresh_tokens WHERE family_id = ?1",
                        params![replay_family],
                    )
                    .map_err(sqlite_error)?;
                    conn.execute_batch("COMMIT").map_err(sqlite_error)?;
                    return Err(AuthError::InvalidGrant(
                        "refresh token replay detected; token family revoked".to_string(),
                    ));
                }
                conn.execute_batch("ROLLBACK").map_err(sqlite_error)?;
                return Ok(None);
            };

            conn.execute(
                "UPDATE used_refresh_tokens
                 SET expires_at = MAX(expires_at, ?1)
                 WHERE family_id = ?2",
                params![family_expires_at, family_id],
            )
            .map_err(sqlite_error)?;
            conn.execute(
                "INSERT INTO used_refresh_tokens (
                    refresh_token_hash, family_id, client_id,
                    token_endpoint_auth_method, used_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    old_hash,
                    family_id,
                    replay_client_id,
                    replay_auth_method,
                    now,
                    family_expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            conn.execute(
                "DELETE FROM refresh_tokens
                 WHERE refresh_token_hash = ?1 AND family_id = ?2",
                params![old_hash, family_id],
            )
            .map_err(sqlite_error)?;
            let insert_result = conn.execute(
                "INSERT INTO refresh_tokens (
                    refresh_token_hash, family_id, client_id, subject, resource, scope,
                    provider_refresh_token, created_at, expires_at, provider,
                    token_endpoint_auth_method
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    new_hash,
                    family_id,
                    new_token.client_id,
                    new_token.subject,
                    new_token.resource,
                    new_token.scope,
                    encrypted_provider_rt,
                    new_token.created_at,
                    new_token.expires_at,
                    new_token.provider,
                    new_token.token_endpoint_auth_method,
                ],
            );
            match insert_result {
                Ok(_) => {
                    conn.execute_batch("COMMIT").map_err(sqlite_error)?;
                    Ok(Some(new_token))
                }
                Err(error) => {
                    drop(conn.execute_batch("ROLLBACK"));
                    Err(sqlite_error(error))
                }
            }
        })
        .await
    }

    pub async fn find_refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<Option<RefreshTokenRow>, AuthError> {
        let hash = hash_token(refresh_token);
        // Keep the plaintext value in memory so the caller receives a row with
        // `refresh_token` populated (the DB never stores it).
        let plaintext = refresh_token.to_string();
        let now = now_unix();
        let enc_key = self.enc_key.clone();
        self.with_conn(move |conn| {
            let row = conn
                .query_row(
                    "SELECT client_id, subject, scope,
                        provider_refresh_token, created_at, expires_at, resource, provider,
                        token_endpoint_auth_method
                 FROM refresh_tokens
                 WHERE refresh_token_hash = ?1
                   AND expires_at > ?2",
                    params![hash, now],
                    |row| {
                        Ok(RefreshTokenRow {
                            refresh_token: plaintext.clone(),
                            client_id: row.get(0)?,
                            subject: row.get(1)?,
                            scope: row.get(2)?,
                            provider_refresh_token: row.get(3)?,
                            created_at: row.get(4)?,
                            expires_at: row.get(5)?,
                            resource: row.get(6).unwrap_or_default(),
                            provider: row.get(7)?,
                            token_endpoint_auth_method: row.get(8)?,
                        })
                    },
                )
                .optional()
                .map_err(sqlite_error)?;

            // Decrypt provider_refresh_token if present and an enc key is
            // configured.  maybe_decrypt_bound is a no-op for plaintext
            // values, so this is safe to call unconditionally once a row is
            // found.  The AAD re-derives the row identity from the same hash
            // used for the lookup, so a ciphertext transplanted onto a
            // different row fails authentication here.
            match row {
                Some(mut r) => {
                    if let Some(raw) = r.provider_refresh_token.as_deref() {
                        r.provider_refresh_token = Some(maybe_decrypt_bound(
                            enc_key.as_deref(),
                            raw,
                            &refresh_token_aad(&hash),
                        )?);
                    }
                    Ok(Some(r))
                }
                None => Ok(None),
            }
        })
        .await
    }

    /// Look up a refresh token for a token-endpoint use. A spent token from the
    /// same client is treated as a replay signal: the active member of its
    /// rotation family is revoked atomically before `invalid_grant` is returned.
    pub async fn find_refresh_token_for_use(
        &self,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<Option<RefreshTokenRow>, AuthError> {
        if let Some(row) = self.find_refresh_token(refresh_token).await? {
            return Ok(Some(row));
        }

        let token_hash = hash_token(refresh_token);
        let client_id = client_id.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(sqlite_error)?;
            let replay_family = conn
                .query_row(
                    "SELECT family_id FROM used_refresh_tokens
                     WHERE refresh_token_hash = ?1
                       AND client_id = ?2
                       AND expires_at > ?3",
                    params![token_hash, client_id, now],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(sqlite_error)?;
            if let Some(family_id) = replay_family {
                conn.execute(
                    "DELETE FROM refresh_tokens WHERE family_id = ?1",
                    params![family_id],
                )
                .map_err(sqlite_error)?;
                conn.execute_batch("COMMIT").map_err(sqlite_error)?;
                return Err(AuthError::InvalidGrant(
                    "refresh token replay detected; token family revoked".to_string(),
                ));
            }
            conn.execute_batch("ROLLBACK").map_err(sqlite_error)?;
            Ok(None)
        })
        .await
    }

    /// Read the `token_endpoint_auth_method` recorded on an unredeemed
    /// authorization code, without consuming it.
    ///
    /// `Ok(None)` covers both "no such code" and a row written before schema
    /// v5 recorded the method — callers must treat it as "unknown", never as
    /// `"none"`.
    pub async fn auth_code_client_auth_method(
        &self,
        code: &str,
    ) -> Result<Option<String>, AuthError> {
        self.client_auth_method(
            "SELECT token_endpoint_auth_method FROM authorization_codes WHERE code = ?1",
            code.to_string(),
        )
        .await
    }

    /// Read the `token_endpoint_auth_method` recorded on a refresh token.
    ///
    /// Same `Ok(None)` semantics as [`Self::auth_code_client_auth_method`].
    pub async fn refresh_token_client_auth_method(
        &self,
        refresh_token: &str,
    ) -> Result<Option<String>, AuthError> {
        self.client_auth_method(
            "SELECT token_endpoint_auth_method FROM refresh_tokens
             WHERE refresh_token_hash = ?1
             UNION ALL
             SELECT token_endpoint_auth_method FROM used_refresh_tokens
             WHERE refresh_token_hash = ?1
             LIMIT 1",
            hash_token(refresh_token),
        )
        .await
    }

    /// Shared single-column lookup behind the two accessors above. The outer
    /// `Option` (row present?) and the inner one (column non-NULL?) collapse
    /// into one because both mean "resolve the client the way we always did".
    async fn client_auth_method(
        &self,
        sql: &'static str,
        key: String,
    ) -> Result<Option<String>, AuthError> {
        self.with_conn(move |conn| {
            conn.query_row(sql, params![key], |row| row.get::<_, Option<String>>(0))
                .optional()
                .map(Option::flatten)
                .map_err(sqlite_error)
        })
        .await
    }
}
