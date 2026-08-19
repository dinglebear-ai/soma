use axum::http::{HeaderMap, HeaderValue, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::pkcs8::EncodePrivateKey as _;
use jsonwebtoken::jwk::JwkSet;

use crate::authorize::tests::test_auth_state;
use crate::registration::ResolvedClient;
use crate::types::{RegisteredClient, TokenRequest};

use super::{
    CLIENT_ASSERTION_TYPE, adopt_jwt_bearer_assertion, apply_basic_client_credentials,
    authenticate_resolved_client, discard_blank_credentials, extract_assertion_client_id,
};

fn jwt_bearer_request() -> TokenRequest {
    TokenRequest {
        grant_type: super::JWT_BEARER_GRANT_TYPE.to_string(),
        code: None,
        redirect_uri: None,
        client_id: None,
        code_verifier: None,
        resource: None,
        refresh_token: None,
        client_secret: None,
        scope: None,
        client_assertion_type: None,
        client_assertion: None,
        assertion: None,
    }
}

fn remote_assertion_signing_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[11u8; 32])
}

const REMOTE_ASSERTION_KID: &str = "remote-client-kid";

fn remote_assertion_jwks() -> JwkSet {
    let public_key = remote_assertion_signing_key().verifying_key();
    serde_json::from_value(serde_json::json!({
        "keys": [{
            "kty": "OKP",
            "crv": "Ed25519",
            "alg": "EdDSA",
            "use": "sig",
            "kid": REMOTE_ASSERTION_KID,
            "x": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key.as_bytes()),
        }]
    }))
    .unwrap()
}

fn signed_remote_client_assertion(client_id: &str, jti: &str) -> String {
    let now = crate::util::now_unix();
    let claims = serde_json::json!({
        "iss": client_id,
        "sub": client_id,
        "aud": "https://lab.example.com/token",
        "iat": now,
        "exp": now + 120,
        "jti": jti,
    });
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    header.kid = Some(REMOTE_ASSERTION_KID.to_string());
    let der = remote_assertion_signing_key().to_pkcs8_der().unwrap();
    jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_ed_der(der.as_bytes()),
    )
    .unwrap()
}

fn cimd_resolved_client(methods: Vec<&str>) -> ResolvedClient {
    ResolvedClient {
        client: RegisteredClient {
            client_id: "https://client.example/metadata.json".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: 0,
            token_endpoint_auth_method: "private_key_jwt".to_string(),
            jwks: None,
        },
        token_endpoint_auth_methods: methods.into_iter().map(str::to_string).collect(),
        jwks_uri: None,
    }
}

#[tokio::test]
async fn cimd_client_may_use_additional_published_none_auth_method() {
    let state = test_auth_state().await;
    let client = cimd_resolved_client(vec!["private_key_jwt", "none"]);

    let method = authenticate_resolved_client(
        &state,
        "https://client.example/metadata.json",
        client,
        None,
        None,
        None,
    )
    .await
    .expect("published none method must be accepted even when private_key_jwt is preferred");
    assert_eq!(method, "none");
}

#[tokio::test]
async fn cimd_private_key_jwt_uses_cached_remote_jwks_uri() {
    let state = test_auth_state().await;
    let client_id = "https://client.example/metadata.json";
    let jwks_uri = "https://client.example/jwks";
    state
        .cimd_jwks_cache
        .seed_for_test(jwks_uri, remote_assertion_jwks());
    let client = ResolvedClient {
        client: RegisteredClient {
            client_id: client_id.to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: 0,
            token_endpoint_auth_method: "private_key_jwt".to_string(),
            jwks: None,
        },
        token_endpoint_auth_methods: vec!["private_key_jwt".to_string()],
        jwks_uri: Some(jwks_uri.to_string()),
    };
    let assertion = signed_remote_client_assertion(client_id, "remote-jwks-auth-jti");

    let method = authenticate_resolved_client(
        &state,
        client_id,
        client,
        None,
        Some(CLIENT_ASSERTION_TYPE),
        Some(&assertion),
    )
    .await
    .expect("cached remote JWKS must authenticate private_key_jwt");
    assert_eq!(method, "private_key_jwt");
}

#[tokio::test]
async fn cimd_preferred_private_key_method_is_not_silently_downgraded() {
    let state = test_auth_state().await;
    let client = cimd_resolved_client(Vec::new());

    let error = authenticate_resolved_client(
        &state,
        "https://client.example/metadata.json",
        client,
        None,
        None,
        None,
    )
    .await
    .expect_err("a client that only publishes private_key_jwt must not authenticate as public");
    assert!(matches!(error, crate::error::AuthError::AuthFailed(_)));
}

#[test]
fn basic_client_credentials_decode_form_components() {
    let mut headers = HeaderMap::new();
    let encoded = STANDARD.encode("client%3Aid:secret%20value");
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {encoded}")).expect("header"),
    );
    let mut request = TokenRequest {
        grant_type: "client_credentials".to_string(),
        code: None,
        redirect_uri: None,
        client_id: None,
        code_verifier: None,
        resource: None,
        refresh_token: None,
        client_secret: None,
        scope: None,
        client_assertion_type: None,
        client_assertion: None,
        assertion: None,
    };
    apply_basic_client_credentials(&headers, &mut request).expect("basic auth");
    assert_eq!(request.client_id.as_deref(), Some("client:id"));
    assert_eq!(request.client_secret.as_deref(), Some("secret value"));
}

#[test]
fn basic_and_body_credentials_are_rejected_as_ambiguous() {
    let mut headers = HeaderMap::new();
    let encoded = STANDARD.encode("client:secret");
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {encoded}")).expect("header"),
    );
    let mut request = TokenRequest {
        grant_type: "client_credentials".to_string(),
        code: None,
        redirect_uri: None,
        client_id: Some("client".to_string()),
        code_verifier: None,
        resource: None,
        refresh_token: None,
        client_secret: None,
        scope: None,
        client_assertion_type: None,
        client_assertion: None,
        assertion: None,
    };
    assert!(apply_basic_client_credentials(&headers, &mut request).is_err());
}

#[test]
fn assertion_subject_can_supply_client_id() {
    // `alg` must name a real algorithm: `insecure_decode` parses the header
    // before skipping verification, and jsonwebtoken rejects `"none"` outright.
    // The signature segment is never checked here, so it can stay empty.
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA"}"#);
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(br#"{"iss":"client","sub":"client","aud":"x","exp":1,"iat":0,"jti":"j"}"#);
    let assertion = format!("{header}.{claims}.");
    assert_eq!(
        extract_assertion_client_id(Some(&assertion)).as_deref(),
        Some("client")
    );
}

#[test]
fn blank_credential_fields_are_treated_as_absent() {
    let mut request = jwt_bearer_request();
    request.client_secret = Some(String::new());
    request.client_assertion = Some(String::new());
    request.client_assertion_type = Some(String::new());
    request.assertion = Some(String::new());
    request.client_id = Some(String::new());
    discard_blank_credentials(&mut request);
    assert_eq!(request.client_secret, None);
    assert_eq!(request.client_assertion, None);
    assert_eq!(request.client_assertion_type, None);
    assert_eq!(request.assertion, None);
    // `client_id` is not a credential and is left exactly as sent.
    assert_eq!(request.client_id.as_deref(), Some(""));
}

#[test]
fn non_blank_credential_fields_survive_normalization() {
    let mut request = jwt_bearer_request();
    request.client_secret = Some(" ".to_string());
    request.assertion = Some("signed-jwt".to_string());
    discard_blank_credentials(&mut request);
    assert_eq!(request.client_secret.as_deref(), Some(" "));
    assert_eq!(request.assertion.as_deref(), Some("signed-jwt"));
}

#[test]
fn jwt_bearer_assertion_becomes_the_client_assertion() {
    let mut request = jwt_bearer_request();
    request.assertion = Some("signed-jwt".to_string());
    adopt_jwt_bearer_assertion(&mut request).expect("assertion adopted");
    assert_eq!(request.assertion, None);
    assert_eq!(request.client_assertion.as_deref(), Some("signed-jwt"));
    assert_eq!(
        request.client_assertion_type.as_deref(),
        Some(CLIENT_ASSERTION_TYPE)
    );
}

#[test]
fn jwt_bearer_grant_without_an_assertion_is_rejected() {
    let mut request = jwt_bearer_request();
    request.client_secret = Some("machine-secret".to_string());
    let error = adopt_jwt_bearer_assertion(&mut request).expect_err("assertion required");
    // `invalid_request`, not `invalid_client`: the credentials may be fine,
    // the grant itself is incomplete.
    assert!(matches!(error, crate::error::AuthError::Validation(_)));
}

#[test]
fn two_disagreeing_assertions_are_rejected_as_ambiguous() {
    let mut request = jwt_bearer_request();
    request.assertion = Some("grant-jwt".to_string());
    request.client_assertion = Some("credential-jwt".to_string());
    assert!(adopt_jwt_bearer_assertion(&mut request).is_err());
}

#[test]
fn a_repeated_identical_assertion_is_accepted() {
    let mut request = jwt_bearer_request();
    request.assertion = Some("same-jwt".to_string());
    request.client_assertion = Some("same-jwt".to_string());
    adopt_jwt_bearer_assertion(&mut request).expect("identical assertions are not ambiguous");
    assert_eq!(request.client_assertion.as_deref(), Some("same-jwt"));
}

#[test]
fn a_mismatched_client_assertion_type_is_rejected() {
    let mut request = jwt_bearer_request();
    request.assertion = Some("grant-jwt".to_string());
    request.client_assertion_type = Some("urn:example:saml2-bearer".to_string());
    assert!(adopt_jwt_bearer_assertion(&mut request).is_err());
}

#[test]
fn basic_credentials_and_a_grant_assertion_are_rejected_as_ambiguous() {
    let mut headers = HeaderMap::new();
    let encoded = STANDARD.encode("client:secret");
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {encoded}")).expect("header"),
    );
    let mut request = jwt_bearer_request();
    request.assertion = Some("grant-jwt".to_string());
    assert!(apply_basic_client_credentials(&headers, &mut request).is_err());
}
