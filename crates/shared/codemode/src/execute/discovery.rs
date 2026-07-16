use crate::types::{ToolDescriptor, ToolScope};

pub fn visible_tools(entries: &[ToolDescriptor], scope: &ToolScope) -> Vec<ToolDescriptor> {
    entries
        .iter()
        .filter(|entry| scope.allows(&entry.id))
        .cloned()
        .collect()
}
