use super::{append_proxy_suffix, configured_bearer_token};

#[test]
fn append_proxy_suffix_preserves_backend_base_and_query() {
    let mut url = reqwest::Url::parse("http://example.com/mcp").unwrap();

    append_proxy_suffix(&mut url, "/messages", Some("session=1"));

    assert_eq!(url.as_str(), "http://example.com/mcp/messages?session=1");
}

#[test]
fn configured_bearer_token_normalizes_optional_scheme() {
    std::env::set_var("SOMA_TEST_PROXY_TOKEN", "Bearer secret");

    assert_eq!(
        configured_bearer_token("SOMA_TEST_PROXY_TOKEN").as_deref(),
        Some("secret")
    );

    std::env::remove_var("SOMA_TEST_PROXY_TOKEN");
}
