use super::store::{SnippetInfo, SnippetSource};

#[test]
fn snippet_info_serializes_source() {
    let info = SnippetInfo {
        name: "hello".to_string(),
        description: Some("demo".to_string()),
        inputs: Default::default(),
        source: SnippetSource::File,
    };
    let value = serde_json::to_value(info).unwrap();
    assert_eq!(value["source"], "File");
}
