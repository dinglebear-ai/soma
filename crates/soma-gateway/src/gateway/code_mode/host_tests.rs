use super::*;

#[test]
fn ui_link_capture_is_preserved() {
    let mut host = GatewayCodeModeHost::default();
    host.capture_ui_link("ui://axon/search");

    assert_eq!(host.ui_links(), &["ui://axon/search".to_owned()]);
}
