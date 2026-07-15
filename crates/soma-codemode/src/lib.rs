pub mod artifacts;
pub mod broker;
pub mod error;
pub mod execute;
pub mod git;
pub mod host;
pub mod local_provider;
pub mod openapi_feature;
pub mod pool;
pub mod preamble;
pub mod process_tree;
pub mod protocol;
pub mod runner;
pub mod runner_drive;
pub mod runner_exe;
pub mod runner_io;
pub mod schema;
pub mod snippet;
pub mod state;
pub mod types;

mod config;
mod home;
mod javy;
mod normalize;
mod path_safety;
mod redact;
mod shape;
mod trace;
mod truncate;
mod ts_signatures;
mod util;
mod wrapper;

#[cfg(test)]
mod artifacts_tests;
#[cfg(test)]
mod broker_tests;
#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod execute_tests;
#[cfg(test)]
mod git_tests;
#[cfg(test)]
mod home_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod javy_tests;
#[cfg(test)]
mod lib_tests;
#[cfg(test)]
mod local_provider_tests;
#[cfg(test)]
mod normalize_tests;
#[cfg(test)]
mod openapi_feature_tests;
#[cfg(test)]
mod path_safety_tests;
#[cfg(test)]
mod pool_tests;
#[cfg(test)]
mod preamble_tests;
#[cfg(test)]
mod process_tree_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod redact_tests;
#[cfg(test)]
mod runner_drive_tests;
#[cfg(test)]
mod runner_exe_tests;
#[cfg(test)]
mod runner_io_tests;
#[cfg(test)]
mod runner_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
mod snippet_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod trace_tests;
#[cfg(test)]
mod truncate_tests;
#[cfg(test)]
mod ts_signatures_tests;
#[cfg(test)]
mod types_tests;
#[cfg(test)]
mod util_tests;
#[cfg(test)]
mod wrapper_tests;

pub const CRATE_NAME: &str = "soma-codemode";

pub use config::{
    install_call_budget_config_defaults, CodeModeConfig, CodeModeResultShapePolicy,
    SemanticSearchConfig, MAX_SOURCE_BYTES, SERVICE,
};
pub use error::ToolError;
pub use home::{env_non_empty, home_dir, soma_home};
pub use normalize::normalize_user_code;
pub use path_safety::{
    reject_existing_symlink_ancestors, reject_existing_symlinks_in_path, reject_path_traversal,
};
pub use redact::{is_sensitive_key, redact_stdio_args, redact_stdio_value, redact_url};
pub use runner::run_code_mode_runner_stdio_blocking;
pub use shape::CodeModeResultShapeMetadata;
pub use trace::{code_mode_execute_trace, redact_trace_value};
pub use truncate::redact_secret_like_segments;
pub use types::{
    namespaced_tool_id, split_namespaced_id, CodeModeCaller, CodeModeCallerCapabilities,
    CodeModeCatalogKind, CodeModeExecutedCall, CodeModeExecutionError, CodeModeExecutionResponse,
    CodeModeExecutionSource, CodeModeHistory, CodeModeHistoryEntry, CodeModeHistoryKind,
    CodeModeSnippetInputEntry, CodeModeSourceLookup, CodeModeSourceStore, CodeModeSurface,
    ToolDescriptor, ToolScope, UiLink,
};
