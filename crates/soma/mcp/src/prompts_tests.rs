//! Unit tests for src/mcp/prompts.rs

use super::*;
use soma_contracts::providers::{ProviderCatalog, ProviderIdentity, ProviderKind, ProviderPrompt};

#[test]
fn list_prompts_returns_quick_start() {
    let result = list_prompts();
    let names: Vec<&str> = result.prompts.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"quick_start"),
        "expected quick_start prompt"
    );
}

#[test]
fn get_prompt_quick_start_returns_message() {
    let result = get_prompt(rmcp::model::GetPromptRequestParams::new("quick_start"))
        .expect("quick_start should resolve");
    assert!(
        !result.messages.is_empty(),
        "prompt should have at least one message"
    );
}

#[test]
fn get_prompt_unknown_returns_err() {
    let result = get_prompt(rmcp::model::GetPromptRequestParams::new("nonexistent"));
    assert!(result.is_err(), "unknown prompt should return Err");
}

fn prompt_catalog(provider_name: &str, prompt: ProviderPrompt) -> ProviderCatalog {
    ProviderCatalog {
        schema_version: 1,
        provider: ProviderIdentity {
            name: provider_name.to_owned(),
            kind: ProviderKind::StaticRust,
            title: None,
            description: None,
            homepage: None,
            source: None,
            version: None,
            enabled: None,
        },
        tools: Vec::new(),
        prompts: vec![prompt],
        resources: Vec::new(),
        tasks: Vec::new(),
        elicitation: Vec::new(),
        env: Vec::new(),
        capabilities: Default::default(),
        docs: None,
        plugin: None,
        ui: None,
        meta: serde_json::json!({}),
    }
}

fn unscoped_prompt(name: &str, template: &str) -> ProviderPrompt {
    ProviderPrompt {
        name: name.to_owned(),
        description: "Review prompt".to_owned(),
        template: Some(template.to_owned()),
        arguments_schema: None,
        scope: None,
        mcp: None,
        examples: Vec::new(),
    }
}

fn loopback_principal() -> ProviderPrincipal {
    ProviderPrincipal {
        subject: "test".to_owned(),
        scopes: Vec::new(),
    }
}

#[test]
fn provider_prompts_are_listed_and_return_markdown_template() {
    let catalogs = vec![prompt_catalog(
        "review-prompt",
        unscoped_prompt("review", "# Review\n\nCheck correctness."),
    )];

    let prompts = provider_prompts(&catalogs);
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].name, "review");

    let result = get_provider_prompt(
        &catalogs,
        &rmcp::model::GetPromptRequestParams::new("review"),
        ProviderAuthMode::LoopbackDev,
        &loopback_principal(),
    );
    let ProviderPromptLookup::Found(result) = result else {
        panic!("expected provider prompt to resolve");
    };
    let value = serde_json::to_value(result).expect("prompt result should serialize");
    assert_eq!(
        value["messages"][0]["content"]["text"],
        "# Review\n\nCheck correctness."
    );
}

#[test]
fn provider_prompt_without_template_is_not_listed_or_servable() {
    let catalogs = vec![prompt_catalog(
        "reservation-only",
        ProviderPrompt {
            template: None,
            ..unscoped_prompt("reserved", "unused")
        },
    )];

    assert!(
        provider_prompts(&catalogs).is_empty(),
        "a prompt with no template should not be advertised in prompts/list"
    );

    let result = get_provider_prompt(
        &catalogs,
        &rmcp::model::GetPromptRequestParams::new("reserved"),
        ProviderAuthMode::LoopbackDev,
        &loopback_principal(),
    );
    assert!(
        matches!(result, ProviderPromptLookup::NotFound),
        "a template-less prompt should not resolve via prompts/get"
    );
}

#[test]
fn get_provider_prompt_denies_caller_without_required_scope() {
    let catalogs = vec![prompt_catalog(
        "scoped-prompt",
        ProviderPrompt {
            scope: Some("soma:write".to_owned()),
            ..unscoped_prompt("scoped", "content")
        },
    )];

    let result = get_provider_prompt(
        &catalogs,
        &rmcp::model::GetPromptRequestParams::new("scoped"),
        ProviderAuthMode::Mounted,
        &ProviderPrincipal {
            subject: "reader".to_owned(),
            scopes: vec!["soma:read".to_owned()],
        },
    );
    match result {
        ProviderPromptLookup::ScopeDenied { required_scope } => {
            assert_eq!(required_scope, "soma:write");
        }
        _ => panic!("expected scope denial"),
    }
}

#[test]
fn get_provider_prompt_allows_caller_with_satisfying_scope() {
    let catalogs = vec![prompt_catalog(
        "scoped-prompt",
        ProviderPrompt {
            scope: Some("soma:read".to_owned()),
            ..unscoped_prompt("scoped", "content")
        },
    )];

    let result = get_provider_prompt(
        &catalogs,
        &rmcp::model::GetPromptRequestParams::new("scoped"),
        ProviderAuthMode::Mounted,
        &ProviderPrincipal {
            subject: "writer".to_owned(),
            scopes: vec!["soma:write".to_owned()],
        },
    );
    assert!(
        matches!(result, ProviderPromptLookup::Found(_)),
        "write scope should satisfy a read-scoped prompt"
    );
}

#[test]
fn get_provider_prompt_ignores_scope_outside_mounted_auth() {
    let catalogs = vec![prompt_catalog(
        "scoped-prompt",
        ProviderPrompt {
            scope: Some("soma:write".to_owned()),
            ..unscoped_prompt("scoped", "content")
        },
    )];

    let result = get_provider_prompt(
        &catalogs,
        &rmcp::model::GetPromptRequestParams::new("scoped"),
        ProviderAuthMode::LoopbackDev,
        &loopback_principal(),
    );
    assert!(
        matches!(result, ProviderPromptLookup::Found(_)),
        "scope should not be enforced outside Mounted auth, matching tool enforce_scope"
    );
}
