use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::at_rest::TokenEncryptionKey;
use crate::error::AuthError;
use crate::types::{
    AllowedUserRow, AuthorizationCodeRow, AuthorizationRequestRow, RegisteredClient,
};

#[path = "sqlite_assertions.rs"]
mod sqlite_assertions;
#[path = "sqlite_browser_sessions.rs"]
mod sqlite_browser_sessions;
#[path = "sqlite_migrations.rs"]
mod sqlite_migrations;
#[path = "sqlite_refresh_queries.rs"]
mod sqlite_refresh_queries;
#[path = "sqlite_refresh_tokens.rs"]
mod sqlite_refresh_tokens;

use sqlite_migrations::{add_column_if_missing, run_migrations};
#[path = "sqlite_rows.rs"]
mod sqlite_rows;
#[path = "sqlite_upstream_oauth.rs"]
mod sqlite_upstream_oauth;
use sqlite_rows::{row_to_allowed_user, row_to_authorization_code, row_to_authorization_request};

/// Schema version for the `PRAGMA user_version` migration guard.
/// Increment this whenever a migration step is added to `run_migrations`.
const SCHEMA_VERSION: i64 = 7;

use crate::util::{
    ensure_restrictive_permissions, fingerprint, now_unix, set_restrictive_permissions,
};

const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;
const SQLITE_POOL_SIZE: usize = 4;

#[derive(Clone)]
pub struct SqliteStore {
    conns: Arc<Vec<Mutex<Connection>>>,
    next_conn: Arc<AtomicUsize>,
    path: Arc<PathBuf>,
    /// Optional at-rest encryption key for upstream provider refresh tokens.
    enc_key: Option<Arc<TokenEncryptionKey>>,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore")
            .field("path", &self.path)
            .field("enc_key", &self.enc_key.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

impl SqliteStore {
    pub async fn open(path: PathBuf) -> Result<Self, AuthError> {
        Self::open_with_key(path, None).await
    }

    pub async fn open_with_key(
        path: PathBuf,
        enc_key: Option<TokenEncryptionKey>,
    ) -> Result<Self, AuthError> {
        let path_for_open = path.clone();
        let conns = tokio::task::spawn_blocking(move || {
            open_connections(path_for_open.as_path(), SQLITE_POOL_SIZE)
        })
        .await;
        let store = match conns {
            Ok(result) => result,
            Err(error) => Err(AuthError::Storage(format!(
                "sqlite open task failed: {error}"
            ))),
        }
        .map(|conns| Self {
            conns: Arc::new(conns.into_iter().map(Mutex::new).collect()),
            next_conn: Arc::new(AtomicUsize::new(0)),
            path: Arc::new(path),
            enc_key: enc_key.map(Arc::new),
        })?;

        store.cleanup_expired().await?;
        Ok(store)
    }

    pub async fn pragma(&self, name: &str) -> Result<String, AuthError> {
        let pragma = match name {
            "journal_mode" | "busy_timeout" | "foreign_keys" => name.to_string(),
            other => {
                return Err(AuthError::Config(format!(
                    "unsupported pragma query `{other}`"
                )));
            }
        };

        self.with_conn(move |conn| {
            conn.query_row(&format!("PRAGMA {pragma};"), [], |row| {
                row.get::<_, Value>(0)
            })
            .map(|value| match value {
                Value::Text(text) => text,
                Value::Integer(int) => int.to_string(),
                other => format!("{other:?}"),
            })
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn register_client(&self, client: RegisteredClient) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            let redirect_uris = serde_json::to_string(&client.redirect_uris)
                .map_err(|error| AuthError::Storage(format!("serialize redirect_uris: {error}")))?;
            let jwks = client
                .jwks
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| AuthError::Storage(format!("serialize client jwks: {error}")))?;
            conn.execute(
                "INSERT INTO registered_clients (
                    client_id, redirect_uris, created_at, token_endpoint_auth_method, jwks
                 ) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(client_id) DO UPDATE SET
                    redirect_uris = excluded.redirect_uris,
                    created_at = excluded.created_at,
                    token_endpoint_auth_method = excluded.token_endpoint_auth_method,
                    jwks = excluded.jwks",
                params![
                    client.client_id,
                    redirect_uris,
                    client.created_at,
                    client.token_endpoint_auth_method,
                    jwks,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_client(
        &self,
        client_id: &str,
    ) -> Result<Option<RegisteredClient>, AuthError> {
        let client_id = client_id.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT client_id, redirect_uris, created_at, token_endpoint_auth_method, jwks
                 FROM registered_clients
                 WHERE client_id = ?1",
                params![client_id],
                |row| {
                    let redirect_uris: String = row.get(1)?;
                    let redirect_uris = serde_json::from_str(&redirect_uris).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    let jwks: Option<String> = row.get(4)?;
                    let jwks = jwks
                        .map(|value| serde_json::from_str(&value))
                        .transpose()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?;
                    Ok(RegisteredClient {
                        client_id: row.get(0)?,
                        redirect_uris,
                        created_at: row.get(2)?,
                        token_endpoint_auth_method: row.get(3)?,
                        jwks,
                    })
                },
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn insert_authorization_request(
        &self,
        request: AuthorizationRequestRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO authorization_requests (
                    state, client_id, redirect_uri, client_state, resource, scope, provider_code_verifier,
                    code_challenge, code_challenge_method, created_at, expires_at, provider,
                    token_endpoint_auth_method
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    request.state,
                    request.client_id,
                    request.redirect_uri,
                    request.client_state,
                    request.resource,
                    request.scope,
                    request.provider_code_verifier,
                    request.code_challenge,
                    request.code_challenge_method,
                    request.created_at,
                    request.expires_at,
                    request.provider,
                    request.token_endpoint_auth_method,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn take_authorization_request(
        &self,
        state: &str,
    ) -> Result<AuthorizationRequestRow, AuthError> {
        let state = state.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM authorization_requests
                 WHERE state = ?1
                   AND expires_at > ?2
                 RETURNING state, client_id, redirect_uri, client_state, scope, provider_code_verifier,
                           code_challenge, code_challenge_method, created_at, expires_at, resource, provider,
                           token_endpoint_auth_method",
                params![state, now],
                row_to_authorization_request,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => AuthError::InvalidGrant(
                    "authorization state is missing, expired, or already used".to_string(),
                ),
                other => sqlite_error(other),
            })
        })
        .await
    }

    pub async fn insert_auth_code(&self, code: AuthorizationCodeRow) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO authorization_codes (
                    code, client_id, subject, redirect_uri, resource, scope,
                    code_challenge, code_challenge_method, provider_refresh_token,
                    created_at, expires_at, provider, token_endpoint_auth_method
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    code.code,
                    code.client_id,
                    code.subject,
                    code.redirect_uri,
                    code.resource,
                    code.scope,
                    code.code_challenge,
                    code.code_challenge_method,
                    code.provider_refresh_token,
                    code.created_at,
                    code.expires_at,
                    code.provider,
                    code.token_endpoint_auth_method,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn redeem_auth_code(&self, code: &str) -> Result<AuthorizationCodeRow, AuthError> {
        let code = code.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM authorization_codes
                 WHERE code = ?1
                   AND expires_at > ?2
                 RETURNING code, client_id, subject, redirect_uri, scope,
                           code_challenge, code_challenge_method, provider_refresh_token,
                           created_at, expires_at, resource, provider,
                           token_endpoint_auth_method",
                params![code, now],
                row_to_authorization_code,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => AuthError::InvalidGrant(
                    "authorization code is missing, expired, or already redeemed".to_string(),
                ),
                other => sqlite_error(other),
            })
        })
        .await
    }

    /// Run an arbitrary SQL batch against the store — test fixtures only.
    ///
    /// Gated behind `cfg(any(test, debug_assertions))` (deliberately not a
    /// Cargo feature, mirroring `upstream::cache`'s test seam) so an
    /// arbitrary-SQL execution method can never ship in
    /// `--all-features --release` artifacts.
    #[cfg(any(test, debug_assertions))]
    pub async fn execute_test_statement(&self, sql: &str) -> Result<(), AuthError> {
        let sql = sql.to_string();
        self.with_conn(move |conn| conn.execute_batch(&sql).map_err(sqlite_error))
            .await
    }

    pub async fn count_pending_oauth_states(&self) -> Result<usize, AuthError> {
        let now = now_unix();
        self.with_conn(move |conn| {
            let authorization_requests: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM authorization_requests WHERE expires_at > ?1",
                    params![now],
                    |row| row.get(0),
                )
                .map_err(sqlite_error)?;
            let browser_login_states: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM browser_login_states WHERE expires_at > ?1",
                    params![now],
                    |row| row.get(0),
                )
                .map_err(sqlite_error)?;
            let native_authorization_results: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM native_authorization_results WHERE expires_at > ?1",
                    params![now],
                    |row| row.get(0),
                )
                .map_err(sqlite_error)?;
            Ok(
                (authorization_requests + browser_login_states + native_authorization_results)
                    as usize,
            )
        })
        .await
    }

    /// Delete expired rows from all short-lived tables. Also drops upstream OAuth
    /// credential rows whose access token has expired AND have no refresh token
    /// available for re-use (SEC-9). Returns the total number of deleted rows.
    pub async fn cleanup_expired(&self) -> Result<u64, AuthError> {
        let now = now_unix();
        self.with_conn(move |conn| {
            let mut total: u64 = 0;
            for table in [
                "authorization_requests",
                "authorization_codes",
                "refresh_tokens",
                "used_refresh_tokens",
                "browser_sessions",
                "browser_login_states",
                "native_authorization_results",
            ] {
                let deleted = conn
                    .execute(
                        &format!("DELETE FROM {table} WHERE expires_at <= ?1"),
                        params![now],
                    )
                    .map_err(sqlite_error)?;
                total += deleted as u64;
            }
            let deleted = conn
                .execute(
                    "DELETE FROM upstream_oauth_state WHERE expires_at <= ?1",
                    params![now],
                )
                .map_err(sqlite_error)?;
            total += deleted as u64;
            let deleted = conn
                .execute(
                    "DELETE FROM upstream_oauth_credentials
                     WHERE access_token_expires_at <= ?1 AND refresh_token_present = 0",
                    params![now],
                )
                .map_err(sqlite_error)?;
            total += deleted as u64;
            Ok(total)
        })
        .await
    }

    /// Add an email address to the allowlist.
    ///
    /// `email` is normalised to lowercase before storage. Returns
    /// `AuthError::Validation` if the email is already present.
    pub async fn add_allowed_user(
        &self,
        email: &str,
        added_by: &str,
        created_at: i64,
    ) -> Result<(), AuthError> {
        let email = email.to_lowercase();
        let fp = fingerprint(&email);
        let added_by = added_by.to_string();
        self.with_conn(move |conn| {
            let changed = conn
                .execute(
                    "INSERT INTO allowed_users (email, added_by, created_at)
                     VALUES (?1, ?2, ?3)",
                    params![email, added_by, created_at],
                )
                .map_err(|error| match error {
                    rusqlite::Error::SqliteFailure(ref e, _)
                        if e.code == rusqlite::ErrorCode::ConstraintViolation =>
                    {
                        AuthError::Validation(format!(
                            "email fingerprint {fp} is already in the allowlist"
                        ))
                    }
                    other => sqlite_error(other),
                })?;
            debug_assert_eq!(changed, 1);
            Ok(())
        })
        .await
    }

    /// Remove an email address from the allowlist.
    ///
    /// Idempotent: returns `Ok(())` even if the email was not present.
    pub async fn remove_allowed_user(&self, email: &str) -> Result<(), AuthError> {
        let email = email.to_lowercase();
        self.with_conn(move |conn| {
            conn.execute("DELETE FROM allowed_users WHERE email = ?1", params![email])
                .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    /// Return all allowlist rows ordered by `created_at ASC`.
    pub async fn list_allowed_users(&self) -> Result<Vec<AllowedUserRow>, AuthError> {
        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT email, added_by, created_at
                     FROM allowed_users
                     ORDER BY created_at ASC",
                )
                .map_err(sqlite_error)?;
            let rows = stmt
                .query_map([], row_to_allowed_user)
                .map_err(sqlite_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sqlite_error)?;
            Ok(rows)
        })
        .await
    }

    async fn with_conn<T, F>(&self, op: F) -> Result<T, AuthError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, AuthError> + Send + 'static,
    {
        let conns = Arc::clone(&self.conns);
        let path = Arc::clone(&self.path);
        let len = conns.len();
        let idx = self.next_conn.fetch_add(1, Ordering::Relaxed) % len;
        tokio::task::spawn_blocking(move || {
            let mut guard = conns[idx]
                .lock()
                .map_err(|_| AuthError::Storage("sqlite mutex poisoned".to_string()))?;
            validate_or_reopen_connection(&mut guard, path.as_ref())?;
            op(&guard)
        })
        .await
        .map_err(|error| AuthError::Storage(format!("sqlite task failed: {error}")))?
    }

    #[cfg(test)]
    fn connection_count(&self) -> usize {
        self.conns.len()
    }
}

fn open_connections(path: &Path, count: usize) -> Result<Vec<Connection>, AuthError> {
    (0..count).map(|_| open_connection(path)).collect()
}

#[allow(clippy::too_many_lines)]
fn open_connection(path: &Path) -> Result<Connection, AuthError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            AuthError::Storage(format!(
                "create auth database directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }

    let existed = path.exists();
    if existed {
        ensure_restrictive_permissions(path)?;
    }

    let conn = Connection::open(path).map_err(sqlite_error)?;
    conn.busy_timeout(std::time::Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS))
        .map_err(sqlite_error)?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(sqlite_error)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(sqlite_error)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS registered_clients (
            client_id TEXT PRIMARY KEY,
            redirect_uris TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            token_endpoint_auth_method TEXT NOT NULL DEFAULT 'none',
            jwks TEXT
        );
        CREATE TABLE IF NOT EXISTS authorization_requests (
            state TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            redirect_uri TEXT NOT NULL,
            client_state TEXT NOT NULL,
            resource TEXT NOT NULL DEFAULT '',
            scope TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'google',
            provider_code_verifier TEXT NOT NULL,
            code_challenge TEXT NOT NULL,
            code_challenge_method TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            token_endpoint_auth_method TEXT
        );
        CREATE TABLE IF NOT EXISTS authorization_codes (
            code TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            redirect_uri TEXT NOT NULL,
            resource TEXT NOT NULL DEFAULT '',
            scope TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'google',
            code_challenge TEXT NOT NULL,
            code_challenge_method TEXT NOT NULL,
            provider_refresh_token TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            token_endpoint_auth_method TEXT
        );
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            refresh_token_hash TEXT PRIMARY KEY,
            family_id TEXT NOT NULL,
            client_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            resource TEXT NOT NULL DEFAULT '',
            scope TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'google',
            provider_refresh_token TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            token_endpoint_auth_method TEXT
        );
        CREATE TABLE IF NOT EXISTS used_refresh_tokens (
            refresh_token_hash TEXT PRIMARY KEY,
            family_id TEXT NOT NULL,
            client_id TEXT NOT NULL,
            token_endpoint_auth_method TEXT,
            used_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_used_refresh_tokens_family
            ON used_refresh_tokens(family_id);
        CREATE TABLE IF NOT EXISTS browser_sessions (
            session_id TEXT PRIMARY KEY,
            subject TEXT NOT NULL,
            email TEXT,
            csrf_token TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS browser_login_states (
            state TEXT PRIMARY KEY,
            return_to TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'google',
            provider_code_verifier TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS native_authorization_results (
            state TEXT PRIMARY KEY,
            code TEXT NOT NULL,
            error TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS upstream_oauth_credentials (
            upstream_name             TEXT NOT NULL,
            subject                   TEXT NOT NULL,
            issuer                    TEXT NOT NULL DEFAULT '',
            client_id                 TEXT NOT NULL,
            granted_scopes_json       TEXT NOT NULL,
            token_blob                BLOB NOT NULL,
            token_blob_nonce          BLOB NOT NULL,
            token_received_at         INTEGER NOT NULL,
            access_token_expires_at   INTEGER NOT NULL,
            refresh_token_present     INTEGER NOT NULL,
            PRIMARY KEY (upstream_name, subject)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS upstream_oauth_state (
            upstream_name        TEXT NOT NULL,
            subject              TEXT NOT NULL,
            csrf_token           TEXT NOT NULL,
            pkce_verifier        TEXT NOT NULL,
            expected_issuer      TEXT,
            require_issuer       INTEGER NOT NULL DEFAULT 0,
            requested_scopes_json TEXT NOT NULL DEFAULT '[]',
            created_at           INTEGER NOT NULL,
            expires_at      INTEGER NOT NULL,
            PRIMARY KEY (upstream_name, subject, csrf_token)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS upstream_oauth_dynamic_clients (
            upstream_name   TEXT NOT NULL,
            subject         TEXT NOT NULL,
            client_id       TEXT NOT NULL,
            issuer          TEXT NOT NULL DEFAULT '',
            created_at      INTEGER NOT NULL,
            PRIMARY KEY (upstream_name, subject)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS allowed_users (
            email       TEXT PRIMARY KEY NOT NULL,
            added_by    TEXT NOT NULL,
            created_at  INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS assertion_jtis (
            issuer TEXT NOT NULL,
            jti TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            PRIMARY KEY (issuer, jti)
        );
        CREATE INDEX IF NOT EXISTS idx_assertion_jtis_expiry
            ON assertion_jtis(expires_at);",
    )
    .map_err(sqlite_error)?;
    add_column_if_missing(
        &conn,
        "registered_clients",
        "token_endpoint_auth_method",
        "TEXT NOT NULL DEFAULT 'none'",
    )?;
    add_column_if_missing(&conn, "registered_clients", "jwks", "TEXT")?;
    add_column_if_missing(
        &conn,
        "authorization_requests",
        "resource",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    add_column_if_missing(
        &conn,
        "authorization_codes",
        "resource",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    add_column_if_missing(
        &conn,
        "refresh_tokens",
        "resource",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    add_column_if_missing(
        &conn,
        "authorization_requests",
        "provider",
        "TEXT NOT NULL DEFAULT 'google'",
    )?;
    add_column_if_missing(
        &conn,
        "authorization_codes",
        "provider",
        "TEXT NOT NULL DEFAULT 'google'",
    )?;
    add_column_if_missing(
        &conn,
        "refresh_tokens",
        "provider",
        "TEXT NOT NULL DEFAULT 'google'",
    )?;
    add_column_if_missing(
        &conn,
        "browser_login_states",
        "provider",
        "TEXT NOT NULL DEFAULT 'google'",
    )?;

    if !existed {
        set_restrictive_permissions(path)?;
    }
    ensure_restrictive_permissions(path)?;

    run_migrations(&conn)?;

    Ok(conn)
}

/// AAD binding an encrypted `provider_refresh_token` to its row identity.
///
/// The refresh-token hash is the table's primary key and is derivable at
/// both encrypt time (upsert/rotate hash the plaintext token before insert)
/// and decrypt time (lookup is keyed by the same hash), so a ciphertext
/// copied onto a row with a different hash fails authentication.  Mirrors
/// the `key=value` AAD shape used by `upstream::store::credential_aad`.
fn refresh_token_aad(refresh_token_hash: &str) -> Vec<u8> {
    format!("refresh_token_hash={refresh_token_hash}").into_bytes()
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in &digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn validate_or_reopen_connection(conn: &mut Connection, path: &Path) -> Result<(), AuthError> {
    let Err(error) = conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0)) else {
        return Ok(());
    };
    warn!(
        path = %path.display(),
        error = %error,
        "stale sqlite connection detected, reopening"
    );

    *conn = open_connection(path)?;
    conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
        .map(|_| ())
        .map_err(sqlite_error)
}

#[allow(clippy::needless_pass_by_value)]
fn sqlite_error(error: rusqlite::Error) -> AuthError {
    AuthError::Storage(format!("sqlite error: {error}"))
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sqlite_migration_tests.rs"]
mod migration_tests;
