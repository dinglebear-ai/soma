use anyhow::{bail, Context, Result};
use rtemplate_contracts::config::ExampleConfig;
use rtemplate_service::{static_provider_registry, ExampleClient, ExampleService};
use serde_json::{json, Value};
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Write,
    CheckAndWrite,
    Help,
}

impl Mode {
    fn parse(args: &[String], usage: &str) -> Result<Self> {
        let mut check = false;
        let mut write = false;
        for arg in args {
            match arg.as_str() {
                "--check" => check = true,
                "--write" => write = true,
                "--help" | "-h" => {
                    println!("{usage}");
                    return Ok(Self::Help);
                }
                unknown => bail!("unknown option: {unknown}"),
            }
        }
        Ok(match (check, write) {
            (false, false) | (true, false) => Self::Check,
            (false, true) => Self::Write,
            (true, true) => Self::CheckAndWrite,
        })
    }

    fn should_check(self) -> bool {
        matches!(self, Self::Check | Self::CheckAndWrite)
    }

    fn should_write(self) -> bool {
        matches!(self, Self::Write | Self::CheckAndWrite)
    }
}

pub fn check_palette_manifest(args: &[String]) -> Result<()> {
    let mode = Mode::parse(
        args,
        "Usage: cargo xtask check-palette-manifest [--check] [--write]",
    )?;
    let root = std::env::current_dir().context("failed to read cwd")?;
    let rendered = canonical_json(&render_palette_manifest()?)?;
    let out = root.join("docs/generated/palette-manifest.json");

    if mode.should_write() {
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&out, &rendered).with_context(|| format!("failed to write {}", out.display()))?;
        println!("wrote {}", relative_display(&root, &out));
    }

    if mode.should_check() {
        if !out.exists() {
            bail!("docs/generated/palette-manifest.json is missing; run cargo xtask check-palette-manifest --write");
        }
        let current = fs::read_to_string(&out)
            .with_context(|| format!("failed to read {}", out.display()))?;
        if current != rendered {
            bail!("docs/generated/palette-manifest.json is stale; run cargo xtask check-palette-manifest --write");
        }
        println!("Palette manifest is current");
    }
    Ok(())
}

fn render_palette_manifest() -> Result<Value> {
    let client = ExampleClient::new(&ExampleConfig {
        api_url: String::new(),
        api_key: "xtask".to_owned(),
    })?;
    let service = ExampleService::new(client);
    let registry = static_provider_registry(service)?;
    let snapshot = registry.snapshot();
    Ok(json!({
        "schema_version": 1,
        "provider_fingerprint": snapshot.fingerprint,
        "commands": snapshot.action_names(),
        "builtins": {
            "file_explorer": false,
            "github": false,
            "browser": false,
            "terminal": false
        },
        "limits": {
            "max_inline_schema_bytes": 16384,
            "max_examples_per_command": 3
        }
    }))
}

fn canonical_json(value: &Value) -> Result<String> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
