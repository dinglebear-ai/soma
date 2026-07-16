use async_trait::async_trait;
use serde_json::{Value, json};
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderId, ProviderManifest,
    ProviderOutput, ProviderRegistry, validate_manifest_schema, validate_provider_manifest,
    validate_provider_manifest_value,
};

#[derive(Clone)]
struct CatalogProvider(ProviderCatalog);

#[async_trait]
impl Provider for CatalogProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.0.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Err(ProviderError::tool_not_found(call.action))
    }
}

#[test]
fn schema_and_rust_model_round_trip_without_unmodeled_properties() {
    let value = json!({
        "schema_version": 1,
        "provider": {
            "name": "schema-provider",
            "kind": "static-rust",
            "title": "Schema provider",
            "description": "Exercises every optional provider property",
            "homepage": "https://example.com/provider",
            "source": "https://example.com/source",
            "version": "1.2.3",
            "enabled": true
        },
        "tools": [{
            "name": "echo",
            "title": "Echo",
            "description": "Echo a value",
            "input_schema": {"type": "object"},
            "output_schema": {"type": "object"},
            "scope": "provider:read",
            "destructive": false,
            "requires_admin": false,
            "cost": "cheap",
            "env": [],
            "limits": {"timeout_ms": 100, "max_response_bytes": 1024, "max_input_bytes": 512},
            "mcp": {
                "enabled": true,
                "title": "Echo",
                "annotations": {"audience": "user"}
            },
            "rest": {"enabled": true, "method": "POST", "path": "/v1/echo", "tags": ["demo"], "summary": "Echo", "description": "Echo", "deprecated": false, "path_params": {}, "query_params": {}, "request_body_schema": {"type": "object"}},
            "cli": {"enabled": true, "command": "echo", "aliases": ["say"], "about": "Echo", "long_about": "Echo a value", "hidden": false, "flags": [], "default_output": "json", "interactive": false},
            "palette": {"enabled": true, "category": "demo", "icon": "terminal", "tone": "neutral", "arg_mode": "schema", "result_view": "auto", "aurora_blocks": []},
            "ui": {"enabled": true, "aurora_registry_dependencies": [], "shadcn_items": [], "categories": [], "meta": {}},
            "examples": [{"title": "Example", "description": "Demo", "input": {}, "output": {}, "cli": "echo", "rest": {}, "mcp": {}}],
            "meta": {"stable": true}
        }],
        "prompts": [{
            "name": "welcome",
            "description": "Welcome a user",
            "template": "Hello {{name}}",
            "arguments_schema": {"type": "object"},
            "scope": "provider:read",
            "mcp": {"enabled": true, "title": "Welcome", "annotations": {}},
            "examples": []
        }],
        "resources": [{"uri_template": "provider://readme", "name": "readme", "description": "Readme", "mime_type": "text/plain", "scope": "provider:read", "mcp": {"enabled": true, "title": "Readme", "annotations": {}}, "annotations": {} }],
        "tasks": [{"name": "summarize", "description": "Summarize", "input_schema": {"type": "object"}, "output_schema": {"type": "object"}, "scope": "provider:read", "mcp": {"enabled": true, "title": "Summarize", "annotations": {}}, "limits": {}}],
        "elicitation": [{"name": "confirm", "description": "Confirm", "schema": {"type": "object"}, "scope": "provider:write", "mcp": {"enabled": true, "title": "Confirm", "annotations": {}}}],
        "env": [{"name": "API_KEY", "description": "API key", "required": true, "sensitive": true, "server_prefixed": true, "allow_unprefixed": false, "default": "demo"}],
        "capabilities": {
            "filesystem": {"enabled": true, "read_roots": ["/tmp"], "write_roots": []},
            "network": {"enabled": true, "allowed_hosts": ["example.com"]},
            "env": {"enabled": true, "allowed": ["API_KEY"]},
            "terminal": {"enabled": true, "working_dir": "/tmp", "allowlist": []},
            "browser": {"enabled": true, "allowed_origins": ["https://example.com"]},
            "github": {"enabled": true, "allowed_repos": ["owner/repo"], "read_only": true}
        },
        "docs": {"when_to_use": "For demos", "examples": [], "troubleshooting": ["Retry"]},
        "plugin": {"generate_skill": true, "generate_claude": true, "generate_codex": true, "generate_gemini": true, "generate_marketplace": true, "mcp_registration": "none"},
        "ui": {"enabled": true, "aurora_registry_dependencies": [], "shadcn_items": [], "categories": [], "meta": {}},
        "meta": {"owner": "test"}
    });

    let manifest = validate_provider_manifest_value(&value).expect("schema-valid manifest parses");
    let serialized = serde_json::to_value(&manifest).expect("manifest serializes");
    validate_provider_manifest(&manifest).expect("typed manifest remains compatibility-valid");
    let reparsed: ProviderManifest = serde_json::from_value(serialized).expect("manifest reparses");
    assert_eq!(reparsed, manifest);
}

#[test]
fn pre_extraction_catalog_json_and_fingerprint_remain_stable() {
    const FIXTURE: &str = include_str!("fixtures/pre_extraction_catalogs.json");
    const FINGERPRINT: &str =
        "sha256:668634aa1429605d10d20fbaf3ac6b6f798db93f9c97e3fbbe9f253c60c79a55";

    let catalogs: Vec<ProviderCatalog> = serde_json::from_str(FIXTURE).expect("fixture parses");
    let serialized = serde_json::to_vec(&catalogs).expect("catalogs serialize");
    assert_eq!(serialized, FIXTURE.trim().as_bytes());

    let registry = ProviderRegistry::builder()
        .register(CatalogProvider(catalogs[0].clone()))
        .expect("fixture provider registers")
        .build()
        .expect("fixture registry builds");
    assert_eq!(registry.snapshot().fingerprint().as_str(), FINGERPRINT);
}

#[test]
fn omitted_value_fields_preserve_hello_static_catalog_bytes_and_fingerprint() {
    const SOURCE: &str = include_str!("fixtures/hello_static_omitted_fields.json");
    const CATALOGS: &str = include_str!("fixtures/pre_extraction_hello_static_catalogs.json");
    const FINGERPRINT: &str =
        "sha256:9c8df0088541c37abc32eee474988b7d3655eb61c3982ed54115a8d19f658719";

    let catalog: ProviderCatalog = serde_json::from_str(SOURCE).expect("source fixture parses");
    let serialized = serde_json::to_vec(&vec![catalog.clone()]).expect("catalog serializes");
    assert_eq!(serialized, CATALOGS.trim().as_bytes());

    let registry = ProviderRegistry::builder()
        .register(CatalogProvider(catalog))
        .expect("fixture provider registers")
        .build()
        .expect("fixture registry builds");
    assert_eq!(registry.snapshot().fingerprint().as_str(), FINGERPRINT);
}

#[test]
fn typed_registry_registration_enforces_packaged_schema_constraints() {
    let mut manifest = ProviderManifest::new(
        ProviderId::new("typed-provider").expect("valid id"),
        "Typed provider",
        "1.0.0",
    );
    manifest.schema_version = 2;
    manifest.meta = Value::Object(Default::default());

    let error = match ProviderRegistry::builder().register(CatalogProvider(manifest)) {
        Ok(_) => panic!("schema-invalid typed catalog must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "json_schema_failed");
}

fn strict_raw_manifest() -> Value {
    json!({
        "schema_version": 1,
        "provider": {"name": "raw-provider", "kind": "static-rust"},
        "tools": [{
            "name": "echo",
            "description": "Echo",
            "input_schema": {"type": "object"}
        }],
        "prompts": [{
            "name": "welcome",
            "description": "Welcome",
            "mcp": {"enabled": true, "title": "Welcome", "annotations": {}}
        }]
    })
}

#[test]
fn raw_schema_validation_rejects_null_optional_properties() {
    let base = strict_raw_manifest();
    validate_manifest_schema(&base).expect("null-free raw manifest is valid");
    validate_provider_manifest_value(&base).expect("null-free raw manifest parses");

    for pointer in ["/provider/enabled", "/tools/0/mcp"] {
        let mut value = base.clone();
        let (parent, field) = pointer.rsplit_once('/').unwrap();
        value.pointer_mut(parent).unwrap()[field] = Value::Null;
        let error = validate_manifest_schema(&value).expect_err("raw null must be rejected");
        assert_eq!(error.code(), "json_schema_failed", "pointer {pointer}");
        let error =
            validate_provider_manifest_value(&value).expect_err("raw manifest null must fail");
        assert_eq!(error.code(), "json_schema_failed", "pointer {pointer}");
    }
}

#[test]
fn typed_legacy_value_nulls_normalize_but_raw_nulls_remain_strict() {
    let base = json!({
        "schema_version": 1,
        "provider": {"name": "legacy-values", "kind": "static-rust"},
        "tools": [{
            "name": "echo",
            "description": "Echo",
            "input_schema": {"type": "object"},
            "mcp": {"enabled": true, "annotations": {}},
            "rest": {"enabled": true, "path_params": {}, "query_params": {}},
            "ui": {"enabled": true, "meta": {}},
            "meta": {}
        }],
        "resources": [{
            "uri_template": "provider://readme",
            "name": "readme",
            "description": "Readme",
            "mcp": {"enabled": true, "annotations": {}},
            "annotations": {}
        }],
        "ui": {"enabled": true, "meta": {}},
        "meta": {}
    });
    validate_manifest_schema(&base).expect("object-valued baseline is schema-valid");

    let schema_object_pointers = [
        "/meta",
        "/tools/0/meta",
        "/tools/0/mcp/annotations",
        "/tools/0/ui/meta",
        "/resources/0/annotations",
        "/resources/0/mcp/annotations",
        "/ui/meta",
    ];
    for pointer in schema_object_pointers {
        let mut raw = base.clone();
        *raw.pointer_mut(pointer).expect("fixture pointer exists") = Value::Null;
        let error = match validate_manifest_schema(&raw) {
            Ok(()) => panic!("raw pointer {pointer} unexpectedly accepted null"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "json_schema_failed", "raw pointer {pointer}");
    }

    let mut typed: ProviderManifest = serde_json::from_value(base).expect("typed fixture parses");
    typed.meta = Value::Null;
    typed.tools[0].meta = Value::Null;
    typed.tools[0].mcp.as_mut().unwrap().annotations = Value::Null;
    typed.tools[0].rest.as_mut().unwrap().path_params = Value::Null;
    typed.tools[0].rest.as_mut().unwrap().query_params = Value::Null;
    typed.tools[0].ui.as_mut().unwrap().meta = Value::Null;
    typed.resources[0].annotations = Value::Null;
    typed.resources[0].mcp.as_mut().unwrap().annotations = Value::Null;
    typed.ui.as_mut().unwrap().meta = Value::Null;
    validate_provider_manifest(&typed).expect("typed legacy nulls normalize privately");
}

#[test]
fn schema_rejects_prompt_title_and_mcp_icons_absent_from_the_rust_model() {
    let base = strict_raw_manifest();

    let mut prompt_title = base.clone();
    prompt_title["prompts"][0]["title"] = json!("Unmodeled title");
    assert_eq!(
        validate_manifest_schema(&prompt_title).unwrap_err().code(),
        "json_schema_failed"
    );

    let mut icons = base;
    icons["prompts"][0]["mcp"]["icons"] = json!([{"src": "https://example.com/icon.png"}]);
    assert_eq!(
        validate_manifest_schema(&icons).unwrap_err().code(),
        "json_schema_failed"
    );
}
