use super::index::SnippetIndex;
use super::store::{SnippetInfo, SnippetSource};

#[test]
fn snippet_index_lists_by_name() {
    let mut index = SnippetIndex::default();
    index.insert(SnippetInfo {
        name: "demo".to_string(),
        description: None,
        inputs: Default::default(),
        source: SnippetSource::Inline,
    });
    assert!(index.get("demo").is_some());
    assert_eq!(index.list().len(), 1);
}
