use std::path::Path;

use soma_self_update::ArtifactTransportPolicy;

use super::{default_state_file, transport_policy};

#[test]
fn default_state_file_is_a_hidden_sibling_of_the_executable() {
    let state = default_state_file(Path::new("/opt/soma/bin/soma")).unwrap();
    assert_eq!(state, Path::new("/opt/soma/bin/.soma.update-state.json"));
}

#[cfg(unix)]
#[test]
fn default_state_file_rejects_non_utf8_names() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let executable = Path::new(OsStr::from_bytes(b"/opt/soma/bin/\xff"));
    assert!(default_state_file(executable).is_err());
}

#[test]
fn transport_defaults_to_https_only() {
    assert_eq!(transport_policy(false), ArtifactTransportPolicy::HttpsOnly);
    assert_eq!(
        transport_policy(true),
        ArtifactTransportPolicy::HttpsOrLoopbackHttp
    );
}
