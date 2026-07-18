use serde_json::json;
use soma_provider_core::ProviderKind;

use super::catalogs_from_inspection;

#[test]
fn inspection_report_maps_remote_inventory_to_catalog() {
    let report = json!({
        "providers": [
            {
                "name": "ai-tools",
                "kind": "ai-sdk",
                "title": "AI Tools",
                "description": "Remote AI SDK tools",
                "homepage": "https://example.com",
                "source": "remote",
                "version": "1.2.3",
                "enabled": true,
                "tools": [
                    {
                        "name": "brief",
                        "title": "Brief",
                        "description": "Summarize text",
                        "input_schema": {
                            "type": "object",
                            "properties": {
                                "text": {"type": "string"}
                            }
                        },
                        "output_schema": {"type": "object"},
                        "scope": "soma:read",
                        "destructive": true,
                        "requires_admin": true,
                        "cost": "metered",
                        "surfaces": {
                            "mcp": true,
                            "rest": false
                        },
                        "env": [
                            {"name": "API_KEY"}
                        ]
                    }
                ],
                "prompts": [
                    {
                        "name": "quick-start",
                        "description": "Start quickly",
                        "template": "# Quick Start\n\nDo the thing.\n",
                        "arguments_schema": {"type": "object"},
                        "scope": "soma:read",
                        "surfaces": {"mcp": true}
                    }
                ],
                "resources": [
                    {
                        "uri_template": "soma://remote/{id}",
                        "name": "remote-resource",
                        "description": "Remote resource",
                        "mime_type": "application/json",
                        "scope": "soma:read",
                        "surfaces": {"mcp": true},
                        "annotations": {"audience": ["assistant"]}
                    }
                ]
            }
        ]
    });

    let catalogs = catalogs_from_inspection(&report).expect("remote catalogs");

    assert_eq!(catalogs.len(), 1);
    let catalog = &catalogs[0];
    assert_eq!(catalog.provider.name, "ai-tools");
    assert_eq!(catalog.provider.kind, ProviderKind::AiSdk);
    assert_eq!(catalog.provider.title.as_deref(), Some("AI Tools"));
    assert_eq!(catalog.provider.version.as_deref(), Some("1.2.3"));
    assert_eq!(catalog.provider.enabled, Some(true));
    assert_eq!(catalog.meta["remote_catalog"], true);

    let tool = &catalog.tools[0];
    assert_eq!(tool.name, "brief");
    assert_eq!(tool.description, "Summarize text");
    assert_eq!(tool.title.as_deref(), Some("Brief"));
    assert_eq!(tool.scope.as_deref(), Some("soma:read"));
    assert!(tool.destructive);
    assert!(tool.requires_admin);
    assert_eq!(tool.cost.as_deref(), Some("metered"));
    assert_eq!(tool.input_schema["properties"]["text"]["type"], "string");
    assert_eq!(
        tool.output_schema.as_ref().expect("output schema")["type"],
        "object"
    );
    assert_eq!(tool.env[0].name, "API_KEY");
    assert!(tool.mcp.as_ref().expect("mcp overlay").enabled);
    assert!(!tool.rest.as_ref().expect("rest overlay").enabled);
    assert_eq!(tool.meta["remote_catalog"], true);

    let prompt = &catalog.prompts[0];
    assert_eq!(prompt.name, "quick-start");
    assert_eq!(
        prompt.template.as_deref(),
        Some("# Quick Start\n\nDo the thing.\n"),
        "a remote catalog report must round-trip prompt.template — without it, \
         a Markdown provider prompt served locally would be silently dropped by \
         servable_prompts() when the same server runs in remote-adapter mode"
    );
    assert!(prompt.mcp.as_ref().expect("prompt mcp overlay").enabled);

    let resource = &catalog.resources[0];
    assert_eq!(resource.uri_template, "soma://remote/{id}");
    assert_eq!(resource.mime_type.as_deref(), Some("application/json"));
    assert!(resource.mcp.as_ref().expect("resource mcp overlay").enabled);
    assert_eq!(resource.annotations["audience"], json!(["assistant"]));
}

#[test]
fn inspection_report_rejects_unknown_provider_kind() {
    let report = json!({
        "providers": [
            {
                "name": "mystery",
                "kind": "unknown-kind"
            }
        ]
    });

    let error = catalogs_from_inspection(&report).expect_err("unknown kind should fail");

    assert!(error
        .to_string()
        .contains("unknown remote provider kind `unknown-kind`"));
}
