use std::path::PathBuf;

use crate::util::now_unix;

use super::SqliteStore;

/// Regression test proving the `provider` column migration correctly
/// backfills a row that predates the column, not just that a freshly
/// created database's `CREATE TABLE ... provider TEXT NOT NULL DEFAULT
/// 'google'` path works. Hand-writes the pre-migration
/// `authorization_requests` shape (no `provider` column) via a raw
/// `rusqlite::Connection`, inserts one row, closes that connection, then
/// opens the SAME file through the normal `SqliteStore::open` path
/// (which runs `add_column_if_missing` for `provider`) and confirms the
/// pre-existing row reads back with `provider = "google"`.
#[tokio::test]
async fn sqlite_store_backfills_provider_column_on_pre_migration_database() {
    let path = temp_db_path();
    let now = now_unix();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE authorization_requests (
                state TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                client_state TEXT NOT NULL,
                resource TEXT NOT NULL DEFAULT '',
                scope TEXT NOT NULL,
                provider_code_verifier TEXT NOT NULL,
                code_challenge TEXT NOT NULL,
                code_challenge_method TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO authorization_requests (
                state, client_id, redirect_uri, client_state, resource, scope,
                provider_code_verifier, code_challenge, code_challenge_method,
                created_at, expires_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                "pre-migration-state",
                "client-1",
                "http://127.0.0.1:7777/callback",
                "client-state",
                "https://lab.example.com/mcp",
                "lab",
                "verifier",
                "challenge",
                "S256",
                now,
                now + 300,
            ],
        )
        .unwrap();
    }
    crate::util::set_restrictive_permissions(&path).unwrap();

    let store = SqliteStore::open(path).await.unwrap();
    let row = store
        .take_authorization_request("pre-migration-state")
        .await
        .unwrap();
    assert_eq!(
        row.provider, "google",
        "pre-existing row must backfill to the 'google' default"
    );
    assert_eq!(row.client_id, "client-1");
    assert_eq!(row.resource, "https://lab.example.com/mcp");
}

/// Same regression coverage as
/// `sqlite_store_backfills_provider_column_on_pre_migration_database`,
/// but for `refresh_tokens` specifically — the highest-stakes of the
/// remaining three migrated tables, since it feeds
/// `has_any_refresh_token_for_provider` and refresh-grant provider
/// dispatch (`token::refresh_token_grant`). Hand-writes the
/// post-v1/pre-`provider`-column `refresh_tokens` shape (hashed PK
/// already present, no `provider` column) via a raw
/// `rusqlite::Connection`, then confirms `SqliteStore::open` backfills
/// the pre-existing row to `provider = "google"`.
#[tokio::test]
async fn sqlite_store_backfills_provider_column_on_pre_migration_refresh_tokens_table() {
    let path = temp_db_path();
    let now = now_unix();
    let plaintext_token = "pre-migration-refresh-token";
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE refresh_tokens (
                refresh_token_hash TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                subject TEXT NOT NULL,
                resource TEXT NOT NULL DEFAULT '',
                scope TEXT NOT NULL,
                provider_refresh_token TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO refresh_tokens (
                refresh_token_hash, client_id, subject, resource, scope,
                provider_refresh_token, created_at, expires_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                super::hash_token(plaintext_token),
                "client-1",
                "google-user",
                "https://lab.example.com/mcp",
                "lab",
                "provider-refresh-token",
                now,
                now + 3600,
            ],
        )
        .unwrap();
    }
    crate::util::set_restrictive_permissions(&path).unwrap();

    let store = SqliteStore::open(path).await.unwrap();
    let row = store
        .find_refresh_token(plaintext_token)
        .await
        .unwrap()
        .expect("pre-existing refresh token row must still be found by its hash");
    assert_eq!(
        row.provider, "google",
        "pre-existing row must backfill to the 'google' default"
    );
    assert_eq!(row.client_id, "client-1");
    assert_eq!(row.resource, "https://lab.example.com/mcp");
}

fn temp_db_path() -> PathBuf {
    tempfile::tempdir().unwrap().keep().join("auth.db")
}
