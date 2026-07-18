use serde_json::json;

use super::{augment_with_palette_routes, CATALOG_PATH, EXECUTE_PATH, SCHEMA_PATH, SEARCH_PATH};

#[test]
fn inserts_all_four_palette_paths() {
    let mut doc = json!({"paths": {}});
    augment_with_palette_routes(&mut doc);
    let paths = doc["paths"].as_object().unwrap();
    assert!(paths.contains_key(CATALOG_PATH));
    assert!(paths.contains_key(SEARCH_PATH));
    assert!(paths.contains_key(SCHEMA_PATH));
    assert!(paths.contains_key(EXECUTE_PATH));
}

#[test]
fn does_not_overwrite_an_existing_entry() {
    let mut doc = json!({"paths": {CATALOG_PATH: {"get": {"summary": "custom"}}}});
    augment_with_palette_routes(&mut doc);
    assert_eq!(doc["paths"][CATALOG_PATH]["get"]["summary"], "custom");
}

#[test]
fn is_a_no_op_when_paths_is_missing() {
    let mut doc = json!({});
    augment_with_palette_routes(&mut doc);
    assert_eq!(doc, json!({}));
}
