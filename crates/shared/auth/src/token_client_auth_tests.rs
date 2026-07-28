use axum::http::{HeaderMap, HeaderValue, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::types::TokenRequest;

use super::{
    CLIENT_ASSERTION_TYPE, adopt_jwt_bearer_assertion, apply_basic_client_credentials,
    discard_blank_credentials, extract_assertion_client_id,
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
