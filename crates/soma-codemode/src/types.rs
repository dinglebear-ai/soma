pub mod caller;
pub mod catalog;
pub mod history;
pub mod id;
pub mod response;
pub mod scope;

#[cfg(test)]
mod caller_tests;
#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod id_tests;
#[cfg(test)]
mod response_tests;
#[cfg(test)]
mod scope_tests;

pub use caller::{CodeModeCaller, CodeModeCallerCapabilities, CodeModeSurface};
pub use catalog::{
    destructive_permitted, CodeModeCatalogKind, CodeModeSnippetInputEntry, ToolDescriptor, UiLink,
};
pub use history::{CodeModeHistory, CodeModeHistoryEntry, CodeModeHistoryKind};
pub use id::{namespaced_tool_id, split_namespaced_id, CodeModeToolId, CodeModeToolRef};
pub use response::{
    CodeModeExecutedCall, CodeModeExecutionError, CodeModeExecutionResponse,
    CodeModeExecutionSource, CodeModeSourceLookup, CodeModeSourceStore,
};
pub use scope::ToolScope;
