use super::local::generate_local_provider_js;

#[test]
fn local_provider_js_exposes_state_and_git() {
    let js = generate_local_provider_js();
    assert!(js.contains("codemode.state"));
    assert!(js.contains("codemode.git"));
}
