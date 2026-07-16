use rmcp::ServerHandler;

use crate::testing::loopback_state;

#[test]
fn rmcp_server_constructs_from_loopback_state() {
    let state = loopback_state();
    let _ = super::rmcp_server(state);
}

#[test]
fn server_info_advertises_tools_resources_prompts() {
    let server = super::rmcp_server(loopback_state());
    let info = server.get_info();
    let caps = &info.capabilities;
    assert!(
        caps.tools.is_some(),
        "server must advertise tools capability"
    );
    assert!(
        caps.resources.is_some(),
        "server must advertise resources capability"
    );
    assert!(
        caps.prompts.is_some(),
        "server must advertise prompts capability"
    );
}

#[test]
fn server_info_includes_implementation_metadata() {
    let server = super::rmcp_server(loopback_state());
    let info = server.get_info();

    let name: &str = info.server_info.name.as_ref();
    let version: &str = info.server_info.version.as_ref();
    assert_eq!(name, "soma");
    assert_eq!(version, env!("CARGO_PKG_VERSION"));

    let instructions = info
        .instructions
        .as_deref()
        .expect("server info should include rich client-facing instructions");
    for expected in [
        "Soma",
        "batteries-included RMCP runtime",
        "drop-in providers",
        "tools, prompts, and resources",
        "one action-dispatched `soma` tool",
        "Homepage: https://soma.dinglebear.ai",
        "Repository: https://github.com/jmagar/soma",
        "Node package: soma-rmcp",
        "Binary: soma",
        "Config home: ~/.soma or SOMA_HOME",
        "Author: dinglebear.ai",
    ] {
        assert!(
            instructions.contains(expected),
            "instructions should mention {expected:?}; got {instructions}"
        );
    }
}

#[test]
fn server_info_uses_configured_server_name() {
    let mut state = loopback_state();
    state.config.server_name = "custom-soma".into();
    let server = super::rmcp_server(state);
    let info = server.get_info();

    let name: &str = info.server_info.name.as_ref();
    assert_eq!(name, "custom-soma");
}
