//! MCP prompts for the Soma server.
//!
//! Prompts are pre-canned message templates that MCP clients can invoke.
//! They appear in the "Prompts" section of compatible MCP UIs.
//!
//! **Customize**: replace `quick_start` with prompts relevant to your domain.
//!
//! Beyond the hardcoded `quick_start` prompt below, drop-in providers can
//! also declare prompts (see `provider_prompts`/`get_provider_prompt`) —
//! currently populated by Markdown prompt files
//! (`providers::filesystem::load_markdown_catalog_value`), but any provider
//! manifest kind can declare a `prompts[]` entry with a `template`.

use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, ListPromptsResult, Prompt, PromptMessage, Role,
};
use soma_contracts::{
    actions::scopes_satisfy,
    providers::{ProviderCatalog, ProviderPrompt},
};
use soma_service::{ProviderAuthMode, ProviderPrincipal};

pub(super) fn list_prompts() -> ListPromptsResult {
    ListPromptsResult {
        prompts: vec![Prompt::new(
            "quick_start",
            Some(
                "Check the server status and get a personalised greeting to verify \
                 the MCP connection is working end-to-end.",
            ),
            None,
        )],
        ..Default::default()
    }
}

pub(super) fn get_prompt(request: GetPromptRequestParams) -> anyhow::Result<GetPromptResult> {
    match request.name.as_str() {
        "quick_start" => Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            "Use the soma tool with action=status to check the server is running, \
             then use action=greet with your name to get a personalised greeting. \
             Report back both results.",
        )])
        .with_description("Verify the MCP server is working with a status check and greeting")),
        other => Err(anyhow::anyhow!("unknown prompt: {other}")),
    }
}

/// Result of resolving a provider-declared prompt by name via `prompts/get`.
pub(super) enum ProviderPromptLookup {
    /// No enabled, servable (has a `template`) prompt with this name exists.
    NotFound,
    /// A matching prompt exists but the caller's scopes don't satisfy
    /// `prompt.scope` under `ProviderAuthMode::Mounted`.
    ScopeDenied {
        required_scope: String,
    },
    Found(GetPromptResult),
}

/// Prompts advertised by drop-in providers (currently just Markdown prompt
/// files — see `providers::filesystem::load_markdown_catalog_value`).
///
/// Only prompts with a `template` are listed — a prompt with no template
/// would resolve to nothing via `get_provider_prompt`, so advertising it
/// would be misleading. This also excludes the built-in `static-rust`
/// provider's `quick_start` reservation entry (which intentionally carries
/// no `template` — see `providers::static_rust::static_catalog`), so the
/// hardcoded `quick_start` prompt below is listed exactly once.
///
/// Does not filter by `prompt.scope` — matches `list_tools`, which also
/// lists every tool regardless of scope. Scope is enforced at the point of
/// use (`get_provider_prompt`), mirroring how tool scope is enforced at
/// `call_tool` rather than `list_tools`.
pub(super) fn provider_prompts(catalogs: &[ProviderCatalog]) -> Vec<Prompt> {
    servable_prompts(catalogs)
        .map(|prompt| Prompt::new(prompt.name.clone(), Some(prompt.description.clone()), None))
        .collect()
}

pub(super) fn get_provider_prompt(
    catalogs: &[ProviderCatalog],
    request: &GetPromptRequestParams,
    auth_mode: ProviderAuthMode,
    principal: &ProviderPrincipal,
) -> ProviderPromptLookup {
    let Some(prompt) = servable_prompts(catalogs).find(|prompt| prompt.name == request.name) else {
        return ProviderPromptLookup::NotFound;
    };

    if matches!(auth_mode, ProviderAuthMode::Mounted) {
        if let Some(scope) = prompt.scope.as_deref() {
            if !scopes_satisfy(&principal.scopes, scope) {
                return ProviderPromptLookup::ScopeDenied {
                    required_scope: scope.to_owned(),
                };
            }
        }
    }

    ProviderPromptLookup::Found(
        GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            // `servable_prompts` guarantees `template.is_some()`.
            prompt.template.clone().unwrap_or_default(),
        )])
        .with_description(prompt.description.clone()),
    )
}

fn servable_prompts(catalogs: &[ProviderCatalog]) -> impl Iterator<Item = &ProviderPrompt> {
    catalogs
        .iter()
        .flat_map(|catalog| &catalog.prompts)
        .filter(|prompt| is_prompt_enabled(prompt) && prompt.template.is_some())
}

fn is_prompt_enabled(prompt: &ProviderPrompt) -> bool {
    prompt.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true)
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
