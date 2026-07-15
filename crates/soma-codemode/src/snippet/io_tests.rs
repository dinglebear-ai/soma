use super::io::snippet_path;

#[test]
fn snippet_path_rejects_escape() {
    let root = std::path::Path::new("/tmp/snippets");
    assert!(snippet_path(root, "demo.js").is_ok());
    assert!(snippet_path(root, "../demo.js").is_err());
}
