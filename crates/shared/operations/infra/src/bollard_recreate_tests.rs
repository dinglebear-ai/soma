use super::*;

#[test]
fn image_split_preserves_registry_ports_and_digests() {
    assert_eq!(
        split_image("registry:5000/app:v1"),
        ("registry:5000/app".into(), Some("v1".into()))
    );
    assert_eq!(
        split_image("app@sha256:deadbeef"),
        ("app@sha256:deadbeef".into(), None)
    );
    assert_eq!(split_image("app"), ("app".into(), Some("latest".into())));
}
