#[test]
fn response_caps_cover_gateway_surfaces() {
    let caps = super::ResponseCaps::default();

    assert!(caps.limit_for(super::CapScope::ToolsList) > 0);
    assert!(caps.limit_for(super::CapScope::ToolsCall) > 0);
    assert!(caps.limit_for(super::CapScope::ResourcesList) > 0);
    assert!(caps.limit_for(super::CapScope::ResourcesRead) > 0);
    assert!(caps.limit_for(super::CapScope::PromptsList) > 0);
    assert!(caps.limit_for(super::CapScope::PromptsGet) > 0);
    assert!(caps.limit_for(super::CapScope::RelayCall) > 0);
    assert!(caps.limit_for(super::CapScope::HttpJson) > 0);
    assert!(caps.limit_for(super::CapScope::HttpSseEvent) > 0);
    assert!(caps.limit_for(super::CapScope::WebSocketFrame) > 0);
    assert!(caps.limit_for(super::CapScope::StdioMessage) > 0);
}

#[test]
fn connected_health_is_the_only_routable_state() {
    assert!(super::UpstreamHealth::Connected.is_routable());
    assert!(!super::UpstreamHealth::Disabled.is_routable());
    assert!(!super::UpstreamHealth::Unsupported {
        reason: "not yet".to_owned()
    }
    .is_routable());
}
