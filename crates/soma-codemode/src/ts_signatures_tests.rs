use serde_json::json;

use super::ts_signatures::{generate_tool_types, namespace_segment, tool_name_to_snake};

#[test]
fn generates_dts_for_required_object_schema() {
    let schema = json!({
        "type": "object",
        "required": ["name"],
        "properties": {"name": {"type": "string"}, "count": {"type": "integer"}}
    });
    let types = generate_tool_types("github", "list.tags", "List tags", Some(&schema), None);
    assert!(types.signature.contains("codemode.github.list_tags"));
    assert!(types.dts.contains("name: string"));
    assert!(types.dts.contains("count?: number"));
}

#[test]
fn sanitizes_names_for_js_identifiers() {
    assert_eq!(tool_name_to_snake("movie.search"), "movie_search");
    assert_eq!(tool_name_to_snake("delete"), "delete_");
    assert_eq!(namespace_segment("search"), "search_");
}
