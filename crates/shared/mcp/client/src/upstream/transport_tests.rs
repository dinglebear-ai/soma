use super::websocket::WebSocketTransportConfig;

#[test]
fn websocket_transport_config_defaults_to_bounded_frames() {
    let config = WebSocketTransportConfig::new("wss://upstream.example/mcp");

    assert_eq!(config.url, "wss://upstream.example/mcp");
    assert_eq!(config.max_frame_size, 128 * 1024);
    assert_eq!(config.max_message_size, 10 * 1024 * 1024);
    assert!(config.authorization.is_none());
}
