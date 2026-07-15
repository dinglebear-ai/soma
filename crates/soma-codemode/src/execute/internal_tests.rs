use super::internal::describe_types;
use crate::types::ToolDescriptor;

#[test]
fn describe_types_returns_signatures() {
    let value = describe_types(&[ToolDescriptor::tool("demo", "call", "", None, None)]);
    assert_eq!(value["tools"][0]["id"], "demo::call");
}
