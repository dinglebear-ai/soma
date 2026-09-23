use super::*;

#[test]
fn canonical_service_keys_and_splits_are_stable() {
    assert_eq!(
        logical_service_key(" Plex Media ").as_deref(),
        Some("plex-media")
    );
    assert_eq!(
        service_instance_key("NAS Host", "Plex").as_deref(),
        Some("nas-host/plex")
    );
    assert_eq!(
        split_service_instance_key("nas/plex"),
        Some(("nas", "plex"))
    );
    assert_eq!(split_service_instance_key("nas/proj/plex"), None);
    assert_eq!(container_key_host("nas:abcdef"), Some("nas"));
    assert_eq!(container_key_host("nas"), None);
}

#[test]
fn legacy_shape_classifier_rejects_urls_and_paths() {
    assert_eq!(
        classify_legacy_shape("nas:plex"),
        Some(LegacyShape::HostService)
    );
    assert_eq!(
        classify_legacy_shape("nas:proj:plex"),
        Some(LegacyShape::HostProjectService)
    );
    assert_eq!(
        classify_legacy_shape("plex/plex/plex"),
        Some(LegacyShape::SlashTriplet)
    );
    assert_eq!(classify_legacy_shape("https://example.test/path"), None);
    assert_eq!(classify_legacy_shape("/mnt/user/media"), None);
}
