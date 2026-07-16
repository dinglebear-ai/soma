use super::*;

fn path(segments: &[&str]) -> ResourcePath {
    parse_resource_path(segments).expect("valid path")
}

#[test]
fn static_path_maps_to_flat_uri() {
    let p = path(&["runbook"]);
    assert!(!p.is_dynamic());
    assert_eq!(p.uri_string(), "soma://resources/runbook");
}

#[test]
fn nested_static_path_preserves_segments() {
    let p = path(&["api", "schema"]);
    assert_eq!(p.uri_string(), "soma://resources/api/schema");
}

#[test]
fn literal_segments_are_slugified_like_prompt_names() {
    let p = path(&["My Api", "Schema_V2"]);
    assert_eq!(p.uri_string(), "soma://resources/my-api/schema-v2");
}

#[test]
fn param_segment_renders_as_braces_and_matches() {
    let p = path(&["service", "[name]"]);
    assert!(p.is_dynamic());
    assert_eq!(p.uri_string(), "soma://resources/service/{name}");

    let params = p.match_segments(&["service", "foo"]).expect("should match");
    assert_eq!(params.get("name"), Some(&"foo".to_owned()));

    assert!(p.match_segments(&["service"]).is_none());
    assert!(p.match_segments(&["service", "foo", "bar"]).is_none());
    assert!(p.match_segments(&["other", "foo"]).is_none());
}

#[test]
fn catch_all_segment_absorbs_remaining_path() {
    let p = path(&["repo", "file", "[...path]"]);
    assert_eq!(p.uri_string(), "soma://resources/repo/file/{path}");

    let params = p
        .match_segments(&["repo", "file", "a", "b", "c"])
        .expect("should match");
    assert_eq!(params.get("path"), Some(&"a/b/c".to_owned()));

    let single = p
        .match_segments(&["repo", "file", "only"])
        .expect("should match single segment");
    assert_eq!(single.get("path"), Some(&"only".to_owned()));

    assert!(p.match_segments(&["repo", "file"]).is_none());
    assert!(p.match_segments(&["repo", "other", "a"]).is_none());
}

#[test]
fn catch_all_must_be_final_segment() {
    let error = parse_resource_path(&["repo", "[...path]", "file"])
        .expect_err("catch-all in the middle must be rejected");
    assert!(error.0.contains("final path segment"));
}

#[test]
fn invalid_param_names_are_rejected() {
    assert!(parse_resource_path(&["[bad-name]"]).is_err());
    assert!(parse_resource_path(&["[9start]"]).is_err());
    assert!(parse_resource_path(&["[...bad-name]"]).is_err());
    assert!(parse_resource_path(&["[valid_name]"]).is_ok());
    assert!(parse_resource_path(&["[_leading]"]).is_ok());
}

#[test]
fn exact_dynamic_path_with_zero_params_matches_only_itself() {
    let p = path(&["status"]);
    assert!(!p.is_dynamic());
    assert_eq!(
        p.match_segments(&["status"]),
        Some(BTreeMap::new()),
        "a zero-param path is exact, matches only the identical request"
    );
    assert!(p.match_segments(&["status", "extra"]).is_none());
}

#[test]
fn identical_shape_with_different_param_names_is_ambiguous() {
    let a = path(&["service", "[name]"]);
    let b = path(&["service", "[id]"]);
    assert!(a.is_ambiguous_with(&b));
}

#[test]
fn different_literal_prefix_is_not_ambiguous() {
    let a = path(&["service", "[name]"]);
    let b = path(&["team", "[id]"]);
    assert!(!a.is_ambiguous_with(&b));
}

#[test]
fn different_segment_count_is_not_ambiguous() {
    let a = path(&["service", "[name]"]);
    let b = path(&["service", "[name]", "detail"]);
    assert!(!a.is_ambiguous_with(&b));
}

#[test]
fn param_and_catch_all_at_the_same_position_are_not_ambiguous() {
    let param = path(&["repo", "[name]"]);
    let catch_all = path(&["repo", "[...name]"]);
    assert!(!param.is_ambiguous_with(&catch_all));
}

#[test]
fn request_segments_strips_prefix_and_query() {
    assert_eq!(
        request_segments("soma://resources/api/schema"),
        Some(vec!["api", "schema"])
    );
    assert_eq!(
        request_segments("soma://resources/status?verbose=true"),
        Some(vec!["status"])
    );
    assert_eq!(request_segments("soma://resources/"), Some(Vec::new()));
    assert_eq!(request_segments("file:///etc/passwd"), None);
}

#[test]
fn slugify_collapses_punctuation_and_trims() {
    assert_eq!(slugify("Code Review"), "code-review");
    assert_eq!(slugify("  --Weird__Name--  "), "weird-name");
    assert_eq!(slugify(""), "");
}
