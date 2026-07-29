use anyhow::{Context, Result, anyhow, bail};
use jsonschema::Draft;

use serde_json::Value;
use std::path::{Path, PathBuf};

const DEFAULT_MANIFEST: &str = "server.json";
const DEFAULT_SCHEMA: &str = "docs/contracts/mcp-server.schema.json";

#[derive(Debug)]
struct Options {
    manifest: PathBuf,
    schema: PathBuf,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            manifest: PathBuf::from(DEFAULT_MANIFEST),
            schema: PathBuf::from(DEFAULT_SCHEMA),
        }
    }
}

impl Options {
    fn parse(args: &[String]) -> Result<Option<Self>> {
        let mut options = Self::default();
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--manifest" => {
                    index += 1;
                    options.manifest = PathBuf::from(
                        args.get(index)
                            .context("--manifest requires a path")?
                            .as_str(),
                    );
                }
                "--schema" => {
                    index += 1;
                    options.schema = PathBuf::from(
                        args.get(index)
                            .context("--schema requires a path")?
                            .as_str(),
                    );
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: cargo xtask check-mcp-registry [--manifest server.json] [--schema docs/contracts/mcp-server.schema.json]"
                    );
                    return Ok(None);
                }
                unknown => bail!("unknown check-mcp-registry option: {unknown}"),
            }
            index += 1;
        }
        Ok(Some(options))
    }
}

pub fn check_cmd(root: &Path, args: &[String]) -> Result<()> {
    let Some(options) = Options::parse(args)? else {
        return Ok(());
    };
    check(root, &options)?;
    println!(
        "MCP registry manifest is valid: {} against {}",
        display_path(root, &options.manifest),
        display_path(root, &options.schema)
    );
    Ok(())
}

pub fn check_default(root: &Path) -> Result<()> {
    check(root, &Options::default())
}

fn check(root: &Path, options: &Options) -> Result<()> {
    let manifest_path = resolve(root, &options.manifest);
    let schema_path = resolve(root, &options.schema);
    let manifest = read_json(&manifest_path)?;
    let schema = read_json(&schema_path)?;
    validate_manifest(&schema, &manifest)
}

fn validate_manifest(schema: &Value, manifest: &Value) -> Result<()> {
    let schema_id = schema
        .get("$id")
        .and_then(Value::as_str)
        .context("MCP registry schema is missing string $id")?;
    let manifest_schema = manifest
        .get("$schema")
        .and_then(Value::as_str)
        .context("server.json is missing string $schema")?;
    if manifest_schema != schema_id {
        bail!(
            "server.json $schema must match vendored MCP registry schema $id: expected {schema_id:?}, found {manifest_schema:?}"
        );
    }

    let compiled = jsonschema::options()
        .with_draft(Draft::Draft7)
        .build(schema)
        .map_err(|error| anyhow!("failed to compile MCP registry schema: {error}"))?;
    let messages = compiled
        .iter_errors(manifest)
        .map(|error| {
            let path = error.instance_path().to_string();
            let path = if path.is_empty() { "<root>" } else { &path };
            format!("{path}: {error}")
        })
        .collect::<Vec<_>>();
    if !messages.is_empty() {
        bail!(
            "server.json does not validate against the MCP registry schema:\n{}",
            messages.join("\n")
        );
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    resolve(root, path)
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "$id": "https://example.test/server.schema.json",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["$schema", "name"],
            "properties": {
                "$schema": {"type": "string"},
                "name": {"type": "string"}
            }
        })
    }

    #[test]
    fn accepts_manifest_matching_schema_id_and_shape() {
        let manifest = json!({
            "$schema": "https://example.test/server.schema.json",
            "name": "dinglebear.ai/soma"
        });
        validate_manifest(&schema(), &manifest).unwrap();
    }

    #[test]
    fn rejects_schema_id_mismatch() {
        let manifest = json!({
            "$schema": "https://example.test/other.schema.json",
            "name": "dinglebear.ai/soma"
        });
        let error = validate_manifest(&schema(), &manifest).unwrap_err();
        assert!(error.to_string().contains("$schema must match"));
    }

    #[test]
    fn rejects_manifest_schema_violations() {
        let manifest = json!({
            "$schema": "https://example.test/server.schema.json",
            "name": 42
        });
        let error = validate_manifest(&schema(), &manifest).unwrap_err();
        assert!(error.to_string().contains("server.json does not validate"));
        assert!(error.to_string().contains("name"));
    }
}
