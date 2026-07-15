use std::collections::BTreeMap;

use super::snippet::store::{SnippetInfo, SnippetInputSpec, SnippetInputType, SnippetSource};
use super::types::ToolDescriptor;

#[test]
fn snippet_descriptor_keeps_input_specs() {
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
    let descriptor = ToolDescriptor::snippet(&info);
    assert_eq!(descriptor.id, "snippet::demo");
    assert_eq!(descriptor.inputs.len(), 1);
}
