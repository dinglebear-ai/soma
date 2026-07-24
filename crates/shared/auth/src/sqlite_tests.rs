use std::path::PathBuf;

use crate::types::{
    AllowedUserRow, AuthorizationCodeRow, BrowserSessionRow, RefreshTokenRow,
    UpstreamOauthCredentialRow, UpstreamOauthStateRow,
};

use crate::util::now_unix;

use super::{SQLITE_POOL_SIZE, SqliteStore};

#[tokio::test]
async fn sqlite_store_enables_wal_and_busy_timeout() {
    let store = temp_store().await;
    assert_eq!(pragma(&store, "journal_mode").await, "wal");
    assert!(pragma_ms(&store, "busy_timeout").await >= 5_000);
}

#[tokio::test]
async fn sqlite_store_opens_multiple_connections() {
    let store = temp_store().await;
    assert_eq!(store.connection_count(), SQLITE_POOL_SIZE);
}

#[tokio::test]
async fn sqlite_store_redeems_auth_code_only_once_under_race() {
    let store = temp_store().await;
    store.insert_auth_code(sample_code()).await.unwrap();
    let (a, b) = tokio::join!(
        store.redeem_auth_code("code-123"),
        store.redeem_auth_code("code-123"),
    );
    assert!(a.is_ok() ^ b.is_ok(), "a={a:?} b={b:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn sqlite_store_refuses_world_readable_database_file() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp_db_path();
    std::fs::write(&path, []).unwrap();
    std::fs::set_permissions(&path, PermissionsExt::from_mode(0o644)).unwrap();
    let err = SqliteStore::open(path).await.unwrap_err();
    assert!(err.to_string().contains("permissions"));
}

#[tokio::test]
async fn sqlite_store_rejects_expired_authorization_code() {
    let store = temp_store().await;
    let mut code = sample_code();
    code.expires_at = now_unix() - 1;
    store.insert_auth_code(code).await.unwrap();
    let err = store.redeem_auth_code("code-123").await.unwrap_err();
    assert!(err.to_string().contains("expired"));
}

#[tokio::test]
async fn sqlite_store_ignores_expired_refresh_token() {
    let store = temp_store().await;
    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "refresh-token".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            resource: "https://lab.example.com/mcp".to_string(),
            scope: "lab".to_string(),
            provider: "google".to_string(),
            provider_refresh_token: Some("provider-refresh".to_string()),
            created_at: now_unix() - 300,
            expires_at: now_unix() - 1,
        })
        .await
        .unwrap();
    assert!(
        store
            .find_refresh_token("refresh-token")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn has_any_refresh_token_reflects_unexpired_rows_only() {
    let store = temp_store().await;
    assert!(!store.has_any_refresh_token().await.unwrap());

    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "expired-refresh".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            resource: "https://lab.example.com/mcp".to_string(),
            scope: "lab".to_string(),
            provider: "google".to_string(),
            provider_refresh_token: None,
            created_at: now_unix() - 300,
            expires_at: now_unix() - 1,
        })
        .await
        .unwrap();
    assert!(
        !store.has_any_refresh_token().await.unwrap(),
        "an expired-only store should not count as having a refresh token"
    );

    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "live-refresh".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            resource: "https://lab.example.com/mcp".to_string(),
            scope: "lab".to_string(),
            provider: "google".to_string(),
            provider_refresh_token: None,
            created_at: now_unix(),
            expires_at: now_unix() + 3600,
        })
        .await
        .unwrap();
    assert!(store.has_any_refresh_token().await.unwrap());
}

#[tokio::test]
async fn sqlite_store_cleanup_expired_removes_stale_rows() {
    let store = temp_store().await;
    let now = now_unix();

    // Insert an expired auth code.
    let mut code = sample_code();
    code.expires_at = now - 10;
    store.insert_auth_code(code).await.unwrap();

    // Insert an expired refresh token.
    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "expired-refresh".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            resource: "https://lab.example.com/mcp".to_string(),
            scope: "lab".to_string(),
            provider: "google".to_string(),
            provider_refresh_token: None,
            created_at: now - 600,
            expires_at: now - 10,
        })
        .await
        .unwrap();

    // Insert an expired authorization request.
    use crate::types::AuthorizationRequestRow;
    store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "expired-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "cs".to_string(),
            resource: "https://lab.example.com/mcp".to_string(),
            scope: "lab".to_string(),
            provider: "google".to_string(),
            provider_code_verifier: "verifier".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            created_at: now - 600,
            expires_at: now - 10,
        })
        .await
        .unwrap();

    // Insert a valid (non-expired) refresh token.
    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "valid-refresh".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            resource: "https://lab.example.com/mcp".to_string(),
            scope: "lab".to_string(),
            provider: "google".to_string(),
            provider_refresh_token: None,
            created_at: now,
            expires_at: now + 3600,
        })
        .await
        .unwrap();

    let deleted = store.cleanup_expired().await.unwrap();
    assert_eq!(deleted, 3, "should delete exactly 3 expired rows");

    // The valid refresh token should still exist.
    assert!(
        store
            .find_refresh_token("valid-refresh")
            .await
            .unwrap()
            .is_some()
    );
}

async fn temp_store() -> SqliteStore {
    SqliteStore::open(temp_db_path()).await.unwrap()
}

async fn pragma(store: &SqliteStore, name: &str) -> String {
    store.pragma(name).await.unwrap()
}

async fn pragma_ms(store: &SqliteStore, name: &str) -> u64 {
    pragma(store, name).await.parse().unwrap()
}

fn temp_db_path() -> PathBuf {
    tempfile::tempdir().unwrap().keep().join("auth.db")
}

fn sample_code() -> AuthorizationCodeRow {
    let now = now_unix();
    AuthorizationCodeRow {
        code: "code-123".to_string(),
        client_id: "client".to_string(),
        subject: "google-user".to_string(),
        redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
        resource: "https://lab.example.com/mcp".to_string(),
        scope: "lab".to_string(),
        provider: "google".to_string(),
        code_challenge: "challenge".to_string(),
        code_challenge_method: "S256".to_string(),
        provider_refresh_token: Some("provider-refresh".to_string()),
        created_at: now,
        expires_at: now + 300,
    }
}

#[tokio::test]
async fn browser_session_round_trip_succeeds() {
    let store = temp_store().await;
    let row = BrowserSessionRow {
        session_id: "sess_123".into(),
        subject: "user_1".into(),
        email: Some("jmagar@example.com".into()),
        csrf_token: "csrf_123".into(),
        created_at: 1,
        expires_at: now_unix() + 9_999,
    };

    store.upsert_browser_session(row.clone()).await.unwrap();
    let fetched = store
        .find_browser_session("sess_123")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(fetched.session_id, row.session_id);
    assert_eq!(fetched.subject, row.subject);
    assert_eq!(fetched.csrf_token, row.csrf_token);
}

fn sample_upstream_credentials() -> UpstreamOauthCredentialRow {
    let now = now_unix();
    UpstreamOauthCredentialRow {
        upstream_name: "acme".to_string(),
        subject: "alice".to_string(),
        client_id: "client-xyz".to_string(),
        granted_scopes_json: "[\"mcp\"]".to_string(),
        token_blob: vec![1, 2, 3, 4],
        token_blob_nonce: vec![0u8; 12],
        token_received_at: now,
        access_token_expires_at: now + 3600,
        refresh_token_present: true,
    }
}

fn sample_upstream_state() -> UpstreamOauthStateRow {
    let now = now_unix();
    UpstreamOauthStateRow {
        upstream_name: "acme".to_string(),
        subject: "alice".to_string(),
        csrf_token: "csrf-1".to_string(),
        pkce_verifier: "verifier-1".to_string(),
        created_at: now,
        expires_at: now + 300,
    }
}

#[tokio::test]
async fn sqlite_store_upsert_upstream_oauth_credentials_round_trip() {
    let store = temp_store().await;
    let row = sample_upstream_credentials();
    store
        .upsert_upstream_oauth_credentials(row.clone())
        .await
        .unwrap();

    let fetched = store
        .find_upstream_oauth_credentials("acme", "alice")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(fetched.upstream_name, row.upstream_name);
    assert_eq!(fetched.subject, row.subject);
    assert_eq!(fetched.client_id, row.client_id);
    assert_eq!(fetched.granted_scopes_json, row.granted_scopes_json);
    assert_eq!(fetched.token_blob, row.token_blob);
    assert_eq!(fetched.token_blob_nonce, row.token_blob_nonce);
    assert_eq!(fetched.token_received_at, row.token_received_at);
    assert_eq!(fetched.access_token_expires_at, row.access_token_expires_at);
    assert_eq!(fetched.refresh_token_present, row.refresh_token_present);
}

#[tokio::test]
async fn sqlite_store_takes_upstream_oauth_state_only_once_under_race() {
    let store = temp_store().await;
    store
        .save_upstream_oauth_state(sample_upstream_state())
        .await
        .unwrap();
    let now = now_unix();
    let (a, b) = tokio::join!(
        store.take_upstream_oauth_state("acme", "alice", "csrf-1", now),
        store.take_upstream_oauth_state("acme", "alice", "csrf-1", now),
    );
    let a_some = matches!(a, Ok(Some(_)));
    let b_some = matches!(b, Ok(Some(_)));
    assert!(
        a_some ^ b_some,
        "exactly one take should win: a={a:?} b={b:?}"
    );
}

#[tokio::test]
async fn sqlite_store_rejects_state_ttl_over_600s() {
    let store = temp_store().await;
    let mut row = sample_upstream_state();
    row.created_at = 1_000;
    row.expires_at = 1_000 + 601;
    let err = store.save_upstream_oauth_state(row).await.unwrap_err();
    assert!(err.to_string().contains("600"));
}

#[tokio::test]
async fn sqlite_store_cleanup_expired_drops_state() {
    let store = temp_store().await;
    let now = now_unix();
    let row = UpstreamOauthStateRow {
        upstream_name: "acme".to_string(),
        subject: "alice".to_string(),
        csrf_token: "csrf-expired".to_string(),
        pkce_verifier: "verifier-expired".to_string(),
        created_at: now - 400,
        expires_at: now - 10,
    };
    store.save_upstream_oauth_state(row).await.unwrap();

    store.cleanup_expired().await.unwrap();

    let fetched = store
        .take_upstream_oauth_state("acme", "alice", "csrf-expired", now)
        .await
        .unwrap();
    assert!(fetched.is_none(), "expired state should be gone");
}

#[tokio::test]
async fn sqlite_store_credentials_isolated_per_subject() {
    let store = temp_store().await;
    let mut row1 = sample_upstream_credentials();
    row1.subject = "alice".to_string();
    let mut row2 = sample_upstream_credentials();
    row2.subject = "bob".to_string();
    store.upsert_upstream_oauth_credentials(row1).await.unwrap();
    store.upsert_upstream_oauth_credentials(row2).await.unwrap();

    store
        .delete_upstream_oauth_credentials("acme", "alice")
        .await
        .unwrap();

    assert!(
        store
            .find_upstream_oauth_credentials("acme", "alice")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .find_upstream_oauth_credentials("acme", "bob")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn sqlite_store_upsert_overwrites_existing_credentials() {
    let store = temp_store().await;
    let row1 = sample_upstream_credentials();
    store.upsert_upstream_oauth_credentials(row1).await.unwrap();

    let mut row2 = sample_upstream_credentials();
    row2.client_id = "client-rotated".to_string();
    row2.token_blob = vec![9, 9, 9];
    store.upsert_upstream_oauth_credentials(row2).await.unwrap();

    let fetched = store
        .find_upstream_oauth_credentials("acme", "alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.client_id, "client-rotated");
    assert_eq!(fetched.token_blob, vec![9, 9, 9]);
}

#[tokio::test]
async fn dynamic_client_registration_round_trip() {
    let store = temp_store().await;

    // Nothing stored yet.
    assert!(
        store
            .find_dynamic_client_registration("acme", "alice")
            .await
            .unwrap()
            .is_none()
    );

    // Save and retrieve.
    store
        .save_dynamic_client_registration("acme", "alice", "client-dyn-1")
        .await
        .unwrap();
    let found = store
        .find_dynamic_client_registration("acme", "alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found, "client-dyn-1");

    // Upsert with a new client_id (server re-registered).
    store
        .save_dynamic_client_registration("acme", "alice", "client-dyn-2")
        .await
        .unwrap();
    let found2 = store
        .find_dynamic_client_registration("acme", "alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found2, "client-dyn-2");

    // Delete and confirm gone; other subjects unaffected.
    store
        .save_dynamic_client_registration("acme", "bob", "client-dyn-bob")
        .await
        .unwrap();
    store
        .delete_dynamic_client_registration("acme", "alice")
        .await
        .unwrap();
    assert!(
        store
            .find_dynamic_client_registration("acme", "alice")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .find_dynamic_client_registration("acme", "bob")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn revoking_browser_session_removes_it() {
    let store = temp_store().await;
    let row = BrowserSessionRow {
        session_id: "sess_123".into(),
        subject: "user_1".into(),
        email: None,
        csrf_token: "csrf_123".into(),
        created_at: 1,
        expires_at: now_unix() + 9_999,
    };

    store.upsert_browser_session(row).await.unwrap();
    store.revoke_browser_session("sess_123").await.unwrap();

    assert!(
        store
            .find_browser_session("sess_123")
            .await
            .unwrap()
            .is_none()
    );
}

// ── allowed_users tests ─────────────────────────────────────────────────

#[tokio::test]
async fn allowed_users_add_and_list() {
    let store = temp_store().await;
    store
        .add_allowed_user("alice@example.com", "admin", now_unix())
        .await
        .unwrap();
    let rows = store.list_allowed_users().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].email, "alice@example.com");
    assert_eq!(rows[0].added_by, "admin");
}

#[tokio::test]
async fn allowed_users_duplicate_returns_validation_error() {
    let store = temp_store().await;
    let now = now_unix();
    store
        .add_allowed_user("bob@example.com", "admin", now)
        .await
        .unwrap();
    let err = store
        .add_allowed_user("bob@example.com", "admin2", now)
        .await
        .unwrap_err();
    assert!(
        matches!(err, crate::error::AuthError::Validation(_)),
        "expected Validation, got {err:?}"
    );
}

#[tokio::test]
async fn allowed_users_input_is_lowercased() {
    let store = temp_store().await;
    store
        .add_allowed_user("Alice@Example.COM", "admin", now_unix())
        .await
        .unwrap();
    let rows = store.list_allowed_users().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].email, "alice@example.com");
}

#[tokio::test]
async fn allowed_users_remove_nonexistent_is_idempotent() {
    let store = temp_store().await;
    // Must not error even when no row exists.
    store
        .remove_allowed_user("nobody@example.com")
        .await
        .unwrap();
}

#[tokio::test]
async fn allowed_users_list_ordered_by_created_at_asc() {
    let store = temp_store().await;
    let base = now_unix();
    store
        .add_allowed_user("third@example.com", "admin", base + 2)
        .await
        .unwrap();
    store
        .add_allowed_user("first@example.com", "admin", base)
        .await
        .unwrap();
    store
        .add_allowed_user("second@example.com", "admin", base + 1)
        .await
        .unwrap();
    let rows = store.list_allowed_users().await.unwrap();
    let emails: Vec<&str> = rows.iter().map(|r| r.email.as_str()).collect();
    assert_eq!(
        emails,
        vec![
            "first@example.com",
            "second@example.com",
            "third@example.com"
        ]
    );
}

#[tokio::test]
async fn allowed_users_schema_bootstrap_is_idempotent() {
    // Open the same file twice; second open must not error.
    let path = temp_db_path();
    let _store1 = SqliteStore::open(path.clone()).await.unwrap();
    let _store2 = SqliteStore::open(path).await.unwrap();
}

fn test_enc_key() -> crate::at_rest::TokenEncryptionKey {
    crate::at_rest::TokenEncryptionKey::from_passphrase("sqlite-at-rest-test-key")
}

fn sample_refresh_row(refresh_token: &str, provider_rt: &str) -> RefreshTokenRow {
    let now = now_unix();
    RefreshTokenRow {
        refresh_token: refresh_token.to_string(),
        client_id: "client".to_string(),
        subject: "google-user".to_string(),
        resource: "https://lab.example.com/mcp".to_string(),
        scope: "lab".to_string(),
        provider: "google".to_string(),
        provider_refresh_token: Some(provider_rt.to_string()),
        created_at: now,
        expires_at: now + 3600,
    }
}

fn raw_provider_rt_column(path: &std::path::Path, hash: &str) -> String {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(
        "SELECT provider_refresh_token FROM refresh_tokens WHERE refresh_token_hash = ?1",
        rusqlite::params![hash],
        |row| row.get(0),
    )
    .unwrap()
}

/// New writes must use the AAD-bound `enc2:` storage format and round-trip
/// back to plaintext through `find_refresh_token`.
#[tokio::test]
async fn sqlite_store_writes_aad_bound_provider_refresh_token() {
    let path = temp_db_path();
    let store = SqliteStore::open_with_key(path.clone(), Some(test_enc_key()))
        .await
        .unwrap();
    store
        .upsert_refresh_token(sample_refresh_row("bound-token", "provider-secret"))
        .await
        .unwrap();

    let stored = raw_provider_rt_column(&path, &super::hash_token("bound-token"));
    assert!(
        stored.starts_with("enc2:"),
        "new writes must carry the AAD-bound sentinel, got: {stored}"
    );

    let row = store
        .find_refresh_token("bound-token")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.provider_refresh_token.as_deref(),
        Some("provider-secret")
    );
}

/// A ciphertext transplanted onto a row with a different refresh-token hash
/// must fail decryption (the AAD re-derivation no longer matches).
#[tokio::test]
async fn sqlite_store_rejects_transplanted_provider_refresh_token() {
    let path = temp_db_path();
    let store = SqliteStore::open_with_key(path.clone(), Some(test_enc_key()))
        .await
        .unwrap();
    store
        .upsert_refresh_token(sample_refresh_row("victim-token", "provider-secret"))
        .await
        .unwrap();

    // Simulate an attacker (or corrupted restore) moving the encrypted blob
    // onto a different row identity by rewriting the primary key.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE refresh_tokens SET refresh_token_hash = ?1 WHERE refresh_token_hash = ?2",
            rusqlite::params![
                super::hash_token("attacker-token"),
                super::hash_token("victim-token"),
            ],
        )
        .unwrap();
    }

    let err = store
        .find_refresh_token("attacker-token")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("decryption failed"),
        "transplanted ciphertext must fail closed, got: {err}"
    );
}

/// Legacy `enc:` rows (written before AAD binding existed) must continue to
/// decrypt through the bound read path.
#[tokio::test]
async fn sqlite_store_decrypts_legacy_unbound_provider_refresh_token() {
    let path = temp_db_path();
    let key = test_enc_key();
    let store = SqliteStore::open_with_key(path.clone(), Some(key.clone()))
        .await
        .unwrap();
    store
        .upsert_refresh_token(sample_refresh_row("legacy-token", "placeholder"))
        .await
        .unwrap();

    // Rewrite the stored column with a legacy unbound ciphertext, as an
    // existing pre-upgrade database would contain.
    let legacy = crate::at_rest::encrypt_provider_token(&key, "legacy-provider-secret").unwrap();
    assert!(legacy.starts_with("enc:"));
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE refresh_tokens SET provider_refresh_token = ?1 WHERE refresh_token_hash = ?2",
            rusqlite::params![legacy, super::hash_token("legacy-token")],
        )
        .unwrap();
    }

    let row = store
        .find_refresh_token("legacy-token")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.provider_refresh_token.as_deref(),
        Some("legacy-provider-secret")
    );
}

// Ensure AllowedUserRow is importable as the right type in tests.
#[allow(dead_code)]
fn _assert_allowed_user_row_type() -> AllowedUserRow {
    AllowedUserRow {
        email: String::new(),
        added_by: String::new(),
        created_at: 0,
    }
}
