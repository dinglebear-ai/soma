use std::collections::BTreeMap;

use super::store::SnippetInfo;

#[derive(Debug, Default, Clone)]
pub struct SnippetIndex {
    snippets: BTreeMap<String, SnippetInfo>,
}

impl SnippetIndex {
    pub fn insert(&mut self, info: SnippetInfo) {
        self.snippets.insert(info.name.clone(), info);
    }

    pub fn get(&self, name: &str) -> Option<&SnippetInfo> {
        self.snippets.get(name)
    }

    pub fn list(&self) -> Vec<&SnippetInfo> {
        self.snippets.values().collect()
    }
}
