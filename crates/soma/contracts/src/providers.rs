//! Compatibility exports for provider metadata now owned by
//! `soma-provider-core`.

pub use soma_provider_core::{
    BrowserCapability, CapabilityGrant, CliOverlay, DocsOverlay, EnvCapability, EnvRequirement,
    FilesystemCapability, GitHubCapability, HostCapabilities, McpOverlay, NetworkCapability,
    PaletteOverlay, PluginOverlay, ProviderCatalog, ProviderElicitation, ProviderExample,
    ProviderIdentity, ProviderKind, ProviderLimits, ProviderManifest, ProviderPrompt,
    ProviderResource, ProviderTask, ProviderTool, RestOverlay, TerminalCapability, ToolSpec,
    UiOverlay,
};
