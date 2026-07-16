use super::names::{namespace_segment, tool_name_to_snake};

#[test]
fn names_match_proxy_identifier_rules() {
    assert_eq!(tool_name_to_snake("movie.search"), "movie_search");
    assert_eq!(namespace_segment("search"), "search_");
}
