use super::{UpstreamOauthConnectionState, UpstreamOauthStatusView};

#[test]
fn status_view_serializes_connection_state_as_snake_case() {
    let view = UpstreamOauthStatusView {
        authenticated: true,
        upstream: "drive".to_owned(),
        state: UpstreamOauthConnectionState::Expiring,
        access_token_expires_at: Some(123),
        seconds_until_expiry: Some(30),
        refresh_token_present: true,
    };

    let json = serde_json::to_value(view).unwrap();

    assert_eq!(json["state"], "expiring");
}
