use super::*;

#[test]
fn compose_contexts_resolve_relative_to_config() {
    assert_eq!(
        resolve_compose_build_context(Path::new("/srv/app/compose.yaml"), "../src").unwrap(),
        PathBuf::from("/srv/src")
    );
    assert_eq!(
        resolve_compose_build_context(Path::new("/srv/app/compose.yaml"), "/opt/src").unwrap(),
        PathBuf::from("/opt/src")
    );
}
