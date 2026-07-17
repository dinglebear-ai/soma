use anyhow::{anyhow, bail, Context, Result};
use jsonschema::Validator;
use serde_json::Value;
use std::{fs, path::Path};

const EXPECTED_INVALID_CODES: &[(&str, &str)] = &[
    ("duplicate-tool.invalid.json", "duplicate_tool_name"),
    ("duplicate-rest-path.invalid.json", "duplicate_rest_route"),
    ("duplicate-cli-alias.invalid.json", "duplicate_cli_command"),
    ("reserved-cli-command.invalid.json", "reserved_cli_command"),
    ("missing-env.invalid.json", "invalid_env_declaration"),
    ("denied-capability.invalid.json", "empty_capability_scope"),
    (
        "duplicate-mcp-primitive.invalid.json",
        "duplicate_mcp_primitive",
    ),
    (
        "generated-doc-prompt-injection.invalid.json",
        "generated_doc_prompt_injection",
    ),
];

pub fn check() -> Result<()> {
    let root = std::env::current_dir().context("failed to read cwd")?;
    let schema_path = root.join("docs/contracts/provider-manifest.schema.json");
    let schema = load_json(&schema_path)?;
    let compiled = jsonschema::options()
        .build(&schema)
        .map_err(|error| anyhow!("failed to compile {}: {error}", schema_path.display()))?;
    let fixtures = root.join("docs/contracts/examples/provider-manifests");

    let valid = [
        "static-rust.valid.json",
        "mcp.valid.json",
        "mcp-rest-opt-in.valid.json",
        "openapi.valid.json",
        "wasm.valid.json",
        "ai-sdk.valid.json",
    ];
    for name in valid {
        let path = fixtures.join(name);
        let payload = load_json(&path)?;
        validate_schema(&compiled, &payload, &path)?;
        soma_domain::provider_validation::validate_provider_manifest_value(&payload)
            .with_context(|| format!("semantic validation failed for {}", path.display()))?;
    }

    for (name, code) in EXPECTED_INVALID_CODES {
        let path = fixtures.join(name);
        let payload = load_json(&path)?;
        validate_schema(&compiled, &payload, &path)?;
        let error = soma_domain::provider_validation::validate_provider_manifest_value(&payload)
            .expect_err("invalid fixture should fail semantic validation");
        if error.code() != *code {
            bail!(
                "{} failed with {}, expected {}",
                path.display(),
                error.code(),
                code
            );
        }
    }

    println!("provider manifest contract fixtures are valid");
    Ok(())
}

fn load_json(path: &Path) -> Result<Value> {
    let raw = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn validate_schema(compiled: &Validator, payload: &Value, path: &Path) -> Result<()> {
    let errors = compiled
        .iter_errors(payload)
        .map(|error| format!("{}: {}", error.instance_path(), error))
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        let details = errors.join("; ");
        bail!(
            "{} failed JSON Schema validation: {}",
            path.display(),
            details
        );
    }
    Ok(())
}
