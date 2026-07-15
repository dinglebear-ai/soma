pub fn generate_local_provider_js() -> &'static str {
    r#"
globalThis.codemode = globalThis.codemode || {};
var codemode = globalThis.codemode;
codemode.state = codemode.state || {};
codemode.git = codemode.git || {};
codemode.state.readFile = (params = {}) => callTool("state::read_file", params);
codemode.state.writeFile = (params = {}) => callTool("state::write_file", params);
codemode.state.status = (params = {}) => callTool("state::status", params);
codemode.git.status = (params = {}) => callTool("git::status", params);
codemode.git.showRef = (params = {}) => callTool("git::show_ref", params);
"#
}
