use crate::types::RefreshTokenRow;
use crate::util::now_unix;

use super::SqliteStore;

async fn store() -> SqliteStore {
    let path = tempfile::tempdir().expect("tempdir").keep().join("auth.db");
    SqliteStore::open(path).await.expect("open sqlite store")
}

#[tokio::test]
async fn assertion_jti_is_consumed_only_once() {
    let store = store().await;
    let now = now_unix();
    assert!(
        store
            .consume_assertion_jti("issuer", "jti-1", now, now + 120, now)
            .await
            .expect("consume first assertion")
    );
    assert!(
        !store
            .consume_assertion_jti("issuer", "jti-1", now, now + 120, now)
            .await
            .expect("reject replay")
    );
}

#[tokio::test]
async fn assertion_jti_rejects_invalid_lifetime_and_allows_expired_reuse() {
    let store = store().await;
    let now = now_unix();
    assert!(
        !store
            .consume_assertion_jti("issuer", "too-long", now, now + 301, now)
            .await
            .expect("reject long assertion")
    );
    assert!(
        store
            .consume_assertion_jti("issuer", "expired", now - 120, now - 1, now - 120)
            .await
            .expect("record assertion before expiry")
    );
    assert!(
        store
            .consume_assertion_jti("issuer", "expired", now, now + 120, now)
            .await
            .expect("expired assertion should be cleaned up")
    );
}

#[tokio::test]
async fn refresh_revocation_is_bound_to_authenticated_client() {
    let store = store().await;
    let now = now_unix();
    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "refresh-secret".to_string(),
            client_id: "client-a".to_string(),
            subject: "machine".to_string(),
            resource: "https://soma.example/mcp".to_string(),
            scope: "soma".to_string(),
            provider: "google".to_string(),
            provider_refresh_token: None,
            created_at: now,
            expires_at: now + 3600,
        })
        .await
        .expect("store refresh token");

    assert!(
        !store
            .revoke_refresh_token("refresh-secret", "client-b")
            .await
            .expect("wrong client must not revoke")
    );
    assert!(
        store
            .find_refresh_token("refresh-secret")
            .await
            .expect("find token")
            .is_some()
    );
    assert!(
        store
            .revoke_refresh_token("refresh-secret", "client-a")
            .await
            .expect("owner revokes token")
    );
    assert!(
        store
            .find_refresh_token("refresh-secret")
            .await
            .expect("find revoked token")
            .is_none()
    );
}
