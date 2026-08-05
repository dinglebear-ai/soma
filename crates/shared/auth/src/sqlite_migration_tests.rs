use std::path::PathBuf;

use crate::types::NativeAuthorizationResultRow;
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
                "https://app.example.com/mcp",
                "app:read",
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
    assert_eq!(row.resource, "https://app.example.com/mcp");
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
                "https://app.example.com/mcp",
                "app:read",
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
    assert_eq!(row.resource, "https://app.example.com/mcp");
}

/// Hand-writes a v4-shaped database (the schema immediately before
/// `token_endpoint_auth_method` was recorded on grants), seeds one
/// authorization code and one refresh token, then opens it through
/// `SqliteStore::open`.
///
/// Two things must hold. The pre-existing rows survive intact — a migration
/// that drops or rewrites live grants logs every user out. And their recorded
/// method reads back as `NULL`, not `'none'`: `NULL` means "issued before this
/// column existed, method unknown", which sends `/token` down the
/// resolve-the-client path it always used. Defaulting to `'none'` would
/// re-label every pre-existing row as a public client, silently downgrading
/// any `private_key_jwt` client whose grants predate v5.
#[tokio::test]
async fn sqlite_store_adds_a_null_client_auth_method_to_pre_v5_rows() {
    let path = temp_db_path();
    let now = now_unix();
    let plaintext_token = "pre-v5-refresh-token";
    write_v4_database(&path, now, plaintext_token);
    crate::util::set_restrictive_permissions(&path).unwrap();

    let store = SqliteStore::open(path.clone()).await.unwrap();

    let refresh = store
        .find_refresh_token(plaintext_token)
        .await
        .unwrap()
        .expect("a pre-v5 refresh token must survive the migration");
    assert_eq!(refresh.client_id, "pre-v5-client");
    assert_eq!(refresh.scope, "app:read");
    assert_eq!(
        refresh.provider_refresh_token.as_deref(),
        Some("provider-refresh-token")
    );
    assert_eq!(
        refresh.token_endpoint_auth_method, None,
        "an unknown method must stay NULL, never default to 'none'"
    );
    assert_eq!(
        store
            .refresh_token_client_auth_method(plaintext_token)
            .await
            .unwrap(),
        None
    );

    let code = store.redeem_auth_code("pre-v5-code").await.unwrap();
    assert_eq!(code.client_id, "pre-v5-client");
    assert_eq!(code.redirect_uri, "http://127.0.0.1:7777/callback");
    assert_eq!(code.token_endpoint_auth_method, None);
    assert_eq!(user_version(&path), 7);
}

/// Re-opening an already-migrated database must be a no-op: the v5 through v7
/// steps use idempotent column and table creation, so a second pass cannot
/// disturb the rows already there.
#[tokio::test]
async fn migrating_to_v7_twice_is_a_no_op() {
    let path = temp_db_path();
    let now = now_unix();
    let plaintext_token = "reopened-refresh-token";
    write_v4_database(&path, now, plaintext_token);
    crate::util::set_restrictive_permissions(&path).unwrap();

    let first = SqliteStore::open(path.clone()).await.unwrap();
    drop(first);
    assert_eq!(user_version(&path), 7);

    let second = SqliteStore::open(path.clone()).await.unwrap();
    let refresh = second
        .find_refresh_token(plaintext_token)
        .await
        .unwrap()
        .expect("re-opening a migrated database must not disturb its rows");
    assert_eq!(refresh.client_id, "pre-v5-client");
    assert_eq!(refresh.token_endpoint_auth_method, None);
    assert_eq!(user_version(&path), 7);
}

#[tokio::test]
async fn schema_v7_adds_native_terminal_error_storage() {
    let path = temp_db_path();
    let now = now_unix();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE native_authorization_results (
                state TEXT PRIMARY KEY,
                code TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
             );
             PRAGMA user_version = 6;",
        )
        .unwrap();
    }
    crate::util::set_restrictive_permissions(&path).unwrap();

    let store = SqliteStore::open(path.clone()).await.unwrap();
    store
        .insert_native_authorization_result(NativeAuthorizationResultRow {
            state: "native-terminal-state-0123456789".to_string(),
            code: None,
            error: Some("access_denied".to_string()),
            created_at: now,
            expires_at: now + 300,
        })
        .await
        .unwrap();
    let result = store
        .take_native_authorization_result("native-terminal-state-0123456789")
        .await
        .unwrap()
        .expect("terminal native result");
    assert_eq!(result.code, None);
    assert_eq!(result.error.as_deref(), Some("access_denied"));
    assert_eq!(user_version(&path), 7);
}

/// The `authorization_codes` and `refresh_tokens` tables exactly as schema v4
/// left them: no `token_endpoint_auth_method` column anywhere, one row in
/// each, and `user_version = 4` so the earlier migrations are correctly
/// treated as already applied.
fn write_v4_database(path: &PathBuf, now: i64, plaintext_token: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE authorization_codes (
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
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE refresh_tokens (
            refresh_token_hash TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            resource TEXT NOT NULL DEFAULT '',
            scope TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'google',
            provider_refresh_token TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        PRAGMA user_version = 4;",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO authorization_codes (
            code, client_id, subject, redirect_uri, resource, scope, provider,
            code_challenge, code_challenge_method, provider_refresh_token,
            created_at, expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            "pre-v5-code",
            "pre-v5-client",
            "google-user",
            "http://127.0.0.1:7777/callback",
            "https://app.example.com/mcp",
            "app:read",
            "google",
            "challenge",
            "S256",
            "provider-refresh-token",
            now,
            now + 300,
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO refresh_tokens (
            refresh_token_hash, client_id, subject, resource, scope, provider,
            provider_refresh_token, created_at, expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            super::hash_token(plaintext_token),
            "pre-v5-client",
            "google-user",
            "https://app.example.com/mcp",
            "app:read",
            "google",
            "provider-refresh-token",
            now,
            now + 3600,
        ],
    )
    .unwrap();
}

fn user_version(path: &PathBuf) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .unwrap()
}

fn temp_db_path() -> PathBuf {
    tempfile::tempdir().unwrap().keep().join("auth.db")
}
