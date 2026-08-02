use std::path::{Path, PathBuf};

use super::*;

fn project() -> ComposeProjectRef {
    ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap()
}

#[test]
fn project_references_are_closed_and_normalized() {
    let project = project();
    assert_eq!(project.name(), "soma");
    assert_eq!(project.config_file(), Path::new("/srv/soma/compose.yaml"));
    assert!(ComposeProjectRef::new("bad name", "/tmp/compose.yml").is_err());
    assert!(ComposeProjectRef::new("soma", "relative.yml").is_err());
    assert!(ComposeProjectRef::new("soma", "/srv/../etc/passwd").is_err());
    assert!(validate_service("api_1.web").is_ok());
    assert!(validate_service("bad service").is_err());
}

#[cfg(unix)]
#[test]
fn project_references_reject_non_utf8_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let mut bytes = b"/tmp/compose-".to_vec();
    bytes.push(0xff);
    assert!(ComposeProjectRef::new("soma", PathBuf::from(OsString::from_vec(bytes))).is_err());
}
