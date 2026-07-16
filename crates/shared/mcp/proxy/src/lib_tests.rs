use soma_mcp_client::upstream::{PromptDescriptor, ResourceDescriptor, ToolDescriptor};

use super::*;

#[test]
fn unique_tools_keep_native_name_unless_reserved_or_duplicated() {
    let routes = tool_routes_from_candidates(
        vec![
            ("alpha".to_owned(), tool("soma")),
            ("alpha".to_owned(), tool("search")),
            ("beta".to_owned(), tool("search")),
        ],
        ["soma"],
    );

    let names = routes
        .into_iter()
        .map(|route| route.name)
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["alpha__soma", "alpha__search", "beta__search"]);

    let routes = tool_routes_from_candidates(
        vec![("alpha".to_owned(), tool("soma"))],
        std::iter::empty::<&str>(),
    );
    assert_eq!(routes[0].name, "soma");
}

#[test]
fn resource_uri_round_trips_native_uris() {
    let native = "test://one/path?x=1&space=a b";
    let route = resource_route("up.one", resource(native));
    let parsed = parse_upstream_resource_uri(&route.uri).expect("synthetic route parses");

    assert_eq!(route.upstream, "up.one");
    assert_eq!(route.native_uri, native);
    assert_eq!(parsed, ("up.one".to_owned(), native.to_owned()));
}

#[test]
fn duplicate_prompts_are_namespaced() {
    let names = prompt_routes_from_candidates(vec![
        ("one".to_owned(), prompt("help")),
        ("two".to_owned(), prompt("help")),
    ])
    .into_iter()
    .map(|route| route.name)
    .collect::<Vec<_>>();

    assert_eq!(names, vec!["one__help", "two__help"]);
}

#[test]
fn exposes_version() {
    assert!(!VERSION.is_empty());
}

fn tool(name: &str) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_owned(),
        description: None,
        input_schema: Some(serde_json::json!({"type": "object"})),
        output_schema: None,
        destructive: false,
    }
}

fn resource(uri: &str) -> ResourceDescriptor {
    ResourceDescriptor {
        uri: uri.to_owned(),
        name: Some("resource".to_owned()),
    }
}

fn prompt(name: &str) -> PromptDescriptor {
    PromptDescriptor {
        name: name.to_owned(),
        description: Some("prompt".to_owned()),
    }
}
