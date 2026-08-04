//! One-time schema migrations keyed by `PRAGMA user_version`, plus the
//! column-addition helper they use. Split out of `sqlite.rs` to stay under
//! the PATTERNS.md module size hard limit.
use rusqlite::{Connection, params};
use tracing::warn;

use crate::error::AuthError;

use super::{SCHEMA_VERSION, hash_token, sqlite_error};

/// One-time migrations keyed by `PRAGMA user_version`.
///
/// Migration 0 → 1: add `refresh_token_hash` to the `refresh_tokens` table
/// (if the table was created with the old `refresh_token TEXT PRIMARY KEY`
/// schema) and backfill SHA-256 hashes for any plaintext rows that pre-date
/// this change.  New databases created with the v1 schema already have
/// `refresh_token_hash` as the PK, so the `ALTER TABLE` step is a no-op in
/// that case.
pub(super) fn run_migrations(conn: &Connection) -> Result<(), AuthError> {
    let current_version: i64 = conn
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .map_err(sqlite_error)?;

    if current_version < 1 {
        // Step 1: add `refresh_token_hash` column if missing (pre-v1 DBs have
        // `refresh_token TEXT PRIMARY KEY` and no hash column).
        let cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(refresh_tokens);")
                .map_err(sqlite_error)?;
            stmt.query_map([], |row| row.get::<_, String>(1))
                .map_err(sqlite_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sqlite_error)?
        };

        if !cols.iter().any(|c| c == "refresh_token_hash") {
            // Old schema: add the column and back-fill SHA-256 hashes.
            conn.execute_batch("ALTER TABLE refresh_tokens ADD COLUMN refresh_token_hash TEXT;")
                .map_err(sqlite_error)?;

            // Back-fill: hash existing plaintext `refresh_token` values.  We
            // can only do this in a SQL-only migration when the hash is
            // computed outside SQLite; instead load all rows, compute hashes
            // in Rust, and update.
            let rows: Vec<(String,)> = {
                let mut stmt = conn
                    .prepare("SELECT refresh_token FROM refresh_tokens WHERE refresh_token_hash IS NULL;")
                    .map_err(sqlite_error)?;
                stmt.query_map([], |row| Ok((row.get::<_, String>(0)?,)))
                    .map_err(sqlite_error)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(sqlite_error)?
            };
            for (plaintext,) in rows {
                let hash = hash_token(&plaintext);
                conn.execute(
                    "UPDATE refresh_tokens SET refresh_token_hash = ?1 WHERE refresh_token = ?2 AND refresh_token_hash IS NULL;",
                    params![hash, plaintext],
                )
                .map_err(sqlite_error)?;
            }

            warn!(
                "migration v1: added refresh_token_hash column and backfilled existing rows — old plaintext tokens invalidated on next rotation"
            );
        }

        // Ensure a UNIQUE index exists on refresh_token_hash so that
        // ON CONFLICT(refresh_token_hash) works correctly on pre-existing
        // databases where the column was added by ALTER TABLE (not declared as
        // PRIMARY KEY).  On new databases the column is already PRIMARY KEY so
        // this index is redundant but harmless.
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_refresh_tokens_hash \
             ON refresh_tokens(refresh_token_hash);",
        )
        .map_err(sqlite_error)?;

        conn.execute_batch("PRAGMA user_version = 1;")
            .map_err(sqlite_error)?;
    }

    if current_version < 2 {
        // Step 2: add `dynamic_client_id` column to `upstream_oauth_state`.
        // This column binds the OAuth client_id used to begin a specific
        // authorization flow to the CSRF state row so that concurrent
        // `begin_authorization` calls for the same upstream+subject can each
        // complete their own callback with the correct client_id.
        add_column_if_missing(conn, "upstream_oauth_state", "dynamic_client_id", "TEXT")?;

        conn.execute_batch("PRAGMA user_version = 2;")
            .map_err(sqlite_error)?;
    }

    if current_version < 3 {
        // Bind durable OAuth state to the authorization-server issuer (SEP-2352)
        // and preserve RFC 9207 callback requirements across restarts. Legacy
        // empty-issuer rows intentionally force a fresh authorization flow.
        add_column_if_missing(
            conn,
            "upstream_oauth_credentials",
            "issuer",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            conn,
            "upstream_oauth_dynamic_clients",
            "issuer",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(conn, "upstream_oauth_state", "expected_issuer", "TEXT")?;
        add_column_if_missing(
            conn,
            "upstream_oauth_state",
            "require_issuer",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            conn,
            "upstream_oauth_state",
            "requested_scopes_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        conn.execute_batch("PRAGMA user_version = 3;")
            .map_err(sqlite_error)?;
    }

    if current_version < 4 {
        add_column_if_missing(
            conn,
            "registered_clients",
            "token_endpoint_auth_method",
            "TEXT NOT NULL DEFAULT 'none'",
        )?;
        add_column_if_missing(conn, "registered_clients", "jwks", "TEXT")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS assertion_jtis (
                issuer TEXT NOT NULL,
                jti TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (issuer, jti)
             );
             CREATE INDEX IF NOT EXISTS idx_assertion_jtis_expiry
                ON assertion_jtis(expires_at);",
        )
        .map_err(sqlite_error)?;
        conn.execute_batch("PRAGMA user_version = 4;")
            .map_err(sqlite_error)?;
    }

    if current_version < 5 {
        // Record the `token_endpoint_auth_method` a grant was issued under so
        // `/token` can authenticate a public client from the grant itself
        // instead of re-resolving the client on every exchange - which, for a
        // CIMD-shaped `client_id`, is a live metadata fetch that made valid
        // refreshes fail whenever the client's own metadata host was down.
        //
        // Deliberately NULLABLE with no default. NULL means "issued before
        // this migration, method unknown", and `/token` resolves those rows
        // exactly as it did before. Defaulting to 'none' would re-label every
        // pre-existing row as a public client, silently downgrading any
        // confidential (`private_key_jwt`) client whose grants predate v5.
        add_column_if_missing(
            conn,
            "authorization_requests",
            "token_endpoint_auth_method",
            "TEXT",
        )?;
        add_column_if_missing(
            conn,
            "authorization_codes",
            "token_endpoint_auth_method",
            "TEXT",
        )?;
        add_column_if_missing(conn, "refresh_tokens", "token_endpoint_auth_method", "TEXT")?;
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
            .map_err(sqlite_error)?;
    }

    Ok(())
}

/// Compute a hex-encoded SHA-256 digest of a token for safe storage.
///
/// The raw token (24+ bytes of random entropy) has sufficient pre-image
/// resistance for SHA-256 to be appropriate here — Argon2 would add
/// per-request latency without a meaningful security benefit.
/// Adds `column` to `table` when a previous schema revision created the
/// table without it. Idempotent: existing columns are left untouched.
pub(super) fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AuthError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sqlite_error)?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(sqlite_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_error)?
        .iter()
        .any(|name| name == column);
    if !exists {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )
        .map_err(sqlite_error)?;
    }
    Ok(())
}
