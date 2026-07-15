use super::*;

#[test]
fn version_is_exported_from_cargo_metadata() {
    assert!(!VERSION.is_empty());
}

#[test]
fn feature_names_document_supported_leaf_surfaces() {
    assert_eq!(
        FEATURE_NAMES,
        [
            "oauth",
            "codemode",
            "openapi",
            "palette",
            "protected-routes"
        ]
    );
}

#[test]
fn openapi_and_palette_imply_codemode_in_feature_metadata() {
    let enabled = [OPENAPI_ENABLED, PALETTE_ENABLED, CODEMODE_ENABLED];

    assert!(!(enabled[0] || enabled[1]) || enabled[2]);
}
