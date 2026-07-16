//! Transport-neutral provider contracts and immutable registry dispatch.
//!
//! `soma-provider-core` defines provider manifests, executable [`ToolSpec`]
//! metadata, validation, provider calls and outputs, and the registry that
//! indexes and dispatches them. It deliberately has no Soma product policy,
//! auth, configuration, process lifecycle, or concrete provider adapters.
//!
//! Build a registry with [`ProviderRegistry::builder`], register implementations
//! of [`Provider`], then dispatch a [`ProviderCall`]. Registry snapshots are
//! immutable and carry a deterministic [`RegistryFingerprint`].

#![forbid(unsafe_code)]

mod call;
mod error;
mod id;
mod manifest;
mod output;
mod provider;
mod registry;
mod surface;
mod validation;

pub use call::ProviderCall;
pub use error::{ProviderError, redact_public};
pub use id::{ProviderId, ProviderIdError};
pub use manifest::*;
pub use output::ProviderOutput;
pub use provider::Provider;
pub use registry::{
    ProviderIndexes, ProviderRegistry, ProviderRegistryBuilder, RegisteredTool,
    RegistryFingerprint, RegistrySnapshot,
};
pub use surface::ProviderSurface;
pub use validation::{
    ProviderValidationError, validate_manifest_schema, validate_provider_manifest,
    validate_provider_manifest_value,
};
