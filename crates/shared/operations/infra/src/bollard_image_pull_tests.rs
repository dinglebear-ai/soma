use serde_json::json;

use super::*;

#[test]
fn pull_frames_normalize_sdk_field_variants() {
    let frame = progress_frame(
        3,
        &json!({
            "status": "Downloading",
            "id": "layer-1",
            "progressDetail": {"current": 5, "total": 10},
            "progress": "[====>]"
        }),
    );
    assert_eq!(frame.sequence, 3);
    assert_eq!(frame.current, Some(5));
    assert_eq!(frame.total, Some(10));
    assert_eq!(frame.id.as_deref(), Some("layer-1"));
}

#[test]
fn image_reference_split_preserves_registry_ports_and_digests() {
    assert_eq!(
        split_image_reference("registry:5000/repo:v1"),
        ("registry:5000/repo".into(), Some("v1".into()))
    );
    assert_eq!(
        split_image_reference("registry:5000/repo"),
        ("registry:5000/repo".into(), None)
    );
    assert_eq!(
        split_image_reference("repo@sha256:abc"),
        ("repo@sha256:abc".into(), None)
    );
}
