use std::{fs, path::Path};

#[test]
fn mcp_provider_is_explicitly_blocked_on_rmcp_2_1_follow_up() {
    let source =
        fs::read_to_string(workspace_root().join("crates/rtemplate-service/src/providers/mcp.rs"))
            .expect("MCP provider source should exist");

    assert!(source.contains("rmcp-template-u4rd"));
    assert!(source.contains("non-executing"));
    assert!(source.contains("roots, sampling"));
    assert!(source.contains("logging"));
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}
