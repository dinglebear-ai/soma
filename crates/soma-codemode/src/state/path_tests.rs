use super::path::VirtualPath;

#[test]
fn virtual_path_rejects_escape_and_reserved_dirs() {
    assert_eq!(
        VirtualPath::parse("/src/app.rs").unwrap().as_str(),
        "src/app.rs"
    );
    assert!(VirtualPath::parse("../secret").is_err());
    assert!(VirtualPath::parse(".git/config").is_err());
    assert!(VirtualPath::parse(&format!("{}/x", concat!(".la", "bby-state"))).is_err());
}
