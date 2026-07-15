use std::collections::BTreeMap;

use serde_json::json;

use super::resolve::bind_snippet_input;
use super::store::{SnippetInfo, SnippetInputSpec, SnippetInputType, SnippetSource};

#[test]
fn bind_snippet_input_requires_declared_values() {
    let info = SnippetInfo {
        name: "demo".to_string(),
        description: None,
        inputs: BTreeMap::from([(
            "name".to_string(),
            SnippetInputSpec {
                input_type: SnippetInputType::String,
                required: true,
                description: None,
            },
        )]),
        source: SnippetSource::Inline,
    };
    assert!(bind_snippet_input(&info, json!({"name": "ok"})).is_ok());
    assert!(bind_snippet_input(&info, json!({})).is_err());
}
