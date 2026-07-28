use axum::http::{HeaderMap, HeaderValue, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::types::TokenRequest;

use super::{apply_basic_client_credentials, extract_assertion_client_id};

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
