pub fn generate_local_provider_js() -> &'static str {
    r#"
globalThis.codemode = globalThis.codemode || {};
var codemode = globalThis.codemode;
codemode.state = codemode.state || {};
codemode.git = codemode.git || {};
codemode.state.readFile = (params = {}) => callTool("state::read_file", params);
codemode.state.writeFile = (params = {}) => callTool("state::write_file", params);
codemode.state.appendFile = (params = {}) => callTool("state::append_file", params);
codemode.state.readJson = (params = {}) => callTool("state::read_json", params);
codemode.state.writeJson = (params = {}) => callTool("state::write_json", params);
codemode.state.hashFile = (params = {}) => callTool("state::hash_file", params);
codemode.state.detectFile = (params = {}) => callTool("state::detect_file", params);
codemode.state.exists = (params = {}) => callTool("state::exists", params);
codemode.state.stat = (params = {}) => callTool("state::stat", params);
codemode.state.mkdir = (params = {}) => callTool("state::mkdir", params);
codemode.state.remove = (params = {}) => callTool("state::remove", params);
codemode.state.copy = (params = {}) => callTool("state::copy", params);
codemode.state.move = (params = {}) => callTool("state::move", params);
codemode.state.walkTree = (params = {}) => callTool("state::walk_tree", params);
codemode.state.list = (params = {}) => callTool("state::list", params);
codemode.state.glob = (params = {}) => callTool("state::glob", params);
codemode.state.searchFiles = (params = {}) => callTool("state::search_files", params);
codemode.state.replaceInFiles = (params = {}) => callTool("state::replace_in_files", params);
codemode.state.planEdits = (params = {}) => callTool("state::plan_edits", params);
codemode.state.applyEditPlan = (params = {}) => callTool("state::apply_edit_plan", params);
codemode.state.status = (params = {}) => callTool("state::status", params);
codemode.git.status = (params = {}) => callTool("git::status", params);
codemode.git.showRef = (params = {}) => callTool("git::show_ref", params);
"#
}
