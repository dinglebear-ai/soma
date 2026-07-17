//! Product configuration, environment loading, and env-var registry for soma.
//!
//! Split out of `soma-contracts` (plan section 3.18): owns Soma's own
//! environment variables, `config.toml` shape, defaults, path resolution, and
//! configuration validation. It does not own engine configuration types
//! themselves (those stay with the engine crates) or business workflows.

pub mod config;
pub mod env_registry;

pub use config::{
    default_data_dir, load_dotenv, AuthConfig, AuthMode, Config, EffectiveRuntimeMode, McpConfig,
    RuntimeMode, SomaConfig,
};
