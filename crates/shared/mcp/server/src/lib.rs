//! Reusable inbound MCP server helpers.

pub mod response_paging;

pub use response_paging::ResponsePageStore;

/// Crate version from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
pub(crate) fn assert_result_has_no_meta(result: &rmcp::model::CallToolResult) {
    assert!(result.meta.is_none(), "result meta should stay empty");
    let serialized = serde_json::to_value(result).expect("result should serialize");
    assert!(
        serialized.get("_meta").is_none(),
        "serialized result included _meta: {serialized}"
    );
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
