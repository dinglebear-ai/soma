use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use soma_contracts::config::SomaConfig;
use soma_contracts::providers::ProviderCatalog;
use soma_service::{dynamic_provider_registry, static_provider_registry, SomaClient, SomaService};
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

pub fn provider_surfaces(args: &[String]) -> Result<()> {
    let mode = Mode::parse(
        args,
        "Usage: cargo xtask generate-provider-surfaces [--check] [--write]",
    )?;
    let root = std::env::current_dir().context("failed to read cwd")?;
    let snapshot = render_provider_snapshot()?;
    let files = [
        (
            root.join("docs/generated/provider-surfaces.json"),
            canonical_json(&snapshot)?,
        ),
        (
            root.join("docs/generated/provider-surfaces.md"),
            render_provider_docs(&snapshot)?,
        ),
        (
            root.join("docs/generated/plugin.json"),
            canonical_json(&render_distribution_plugin(&snapshot))?,
        ),
        (
            root.join(".agents/plugins/marketplace.json"),
            canonical_json(&render_codex_marketplace())?,
        ),
        (
            root.join(".claude-plugin/marketplace.json"),
            canonical_json(&render_claude_marketplace())?,
        ),
    ];

    for (path, content) in files {
        if mode.should_write() {
            write_if_changed(&path, &content)?;
            println!("wrote {}", relative_display(&root, &path));
        }
        if mode.should_check() {
            if !path.exists() {
                bail!(
                    "{} is missing; run cargo xtask generate-provider-surfaces --write",
                    relative_display(&root, &path)
                );
            }
            let current = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            if current != content {
                bail!(
                    "{} is stale; run cargo xtask generate-provider-surfaces --write",
                    relative_display(&root, &path)
                );
            }
        }
    }
    write_or_check_generated_skills(&root, &snapshot, mode)?;
    if mode.should_check() {
        println!("Provider surface artifacts are current");
    }
    Ok(())
}

fn render_palette_manifest() -> Result<Value> {
    let client = SomaClient::new(&SomaConfig {
        api_url: String::new(),
        api_key: "xtask".to_owned(),
    })?;
    let service = SomaService::new(client);
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

fn render_provider_snapshot() -> Result<Value> {
    let provider_dir = provider_dir();
    let client = SomaClient::new(&SomaConfig {
        api_url: String::new(),
        api_key: "xtask".to_owned(),
    })?;
    let service = SomaService::new(client);
    let registry = dynamic_provider_registry(service)?;
    let snapshot = registry.refresh_file_providers()?;
    Ok(json!({
        "schema_version": 1,
        "provider_fingerprint": snapshot.fingerprint,
        "providers": snapshot.catalogs.iter().map(provider_summary).collect::<Vec<_>>(),
        "surfaces": {
            "mcp_actions": surface_actions(&snapshot.catalogs, Surface::Mcp),
            "cli_actions": surface_actions(&snapshot.catalogs, Surface::Cli),
            "cli_commands": cli_commands(&snapshot.catalogs),
            "rest_routes": rest_routes(&snapshot.catalogs),
            "docs": "docs/generated/provider-surfaces.md",
            "plugin": "docs/generated/plugin.json",
            "codex_marketplace": ".agents/plugins/marketplace.json",
            "claude_marketplace": ".claude-plugin/marketplace.json",
            "node_package": "packages/soma-rmcp/package.json",
            "provider_dir": provider_dir.display().to_string(),
            "provider_files": provider_files(&provider_dir)?,
            "generated_skills": generated_skill_paths(&snapshot.catalogs),
            "palette": "deferred until Axon tauri-palette port lands"
        }
    }))
}

fn provider_summary(catalog: &ProviderCatalog) -> Value {
    json!({
        "name": catalog.provider.name,
        "kind": catalog.provider.kind.as_str(),
        "title": catalog.provider.title,
        "description": catalog.provider.description,
        "when_to_use": catalog.docs.as_ref().and_then(|docs| docs.when_to_use.clone()),
        "tools": catalog.tools.iter().map(|tool| json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": tool.input_schema,
            "output_schema": tool.output_schema,
            "scope": tool.scope,
            "destructive": tool.destructive,
            "requires_admin": tool.requires_admin,
            "cost": tool.cost,
            "env": tool.env.iter().map(|env| json!({
                "name": env.name,
                "required": env.required,
                "sensitive": env.sensitive,
                "server_prefixed": env.server_prefixed,
                "allow_unprefixed": env.allow_unprefixed,
                "description": env.description,
            })).collect::<Vec<_>>(),
            "mcp": tool.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true),
            "cli": tool.cli.as_ref().map(|cli| cli.enabled).unwrap_or(false),
            "cli_command": tool.cli.as_ref().filter(|cli| cli.enabled).and_then(|cli| cli.command.clone()).unwrap_or_else(|| if tool.cli.as_ref().map(|cli| cli.enabled).unwrap_or(false) { tool.name.clone() } else { "N/A".to_owned() }),
            "cli_aliases": tool.cli.as_ref().map(|cli| cli.aliases.clone()).unwrap_or_default(),
            "cli_flags": tool.cli.as_ref().map(|cli| cli.flags.clone()).unwrap_or_default(),
            "cli_default_output": tool.cli.as_ref().and_then(|cli| cli.default_output.clone()),
            "cli_usage": tool.meta.get("cli_usage").and_then(Value::as_str).map(ToOwned::to_owned),
            "rest": tool.rest.as_ref().map(|rest| rest.enabled).unwrap_or(false),
            "rest_route": tool.rest.as_ref().filter(|rest| rest.enabled).map(|rest| format!("{} {}", rest.method.as_deref().unwrap_or("POST"), rest.path.clone().unwrap_or_else(|| format!("/v1/{}", tool.name)))).unwrap_or_else(|| "N/A".to_owned()),
            "examples": tool.examples,
            "meta": tool.meta,
        })).collect::<Vec<_>>(),
        "prompts": catalog.prompts.iter().map(|prompt| json!({
            "name": prompt.name,
            "description": prompt.description,
            "arguments_schema": prompt.arguments_schema,
        })).collect::<Vec<_>>(),
        "resources": catalog.resources.iter().map(|resource| json!({
            "name": resource.name,
            "description": resource.description,
            "uri_template": resource.uri_template,
            "mime_type": resource.mime_type,
        })).collect::<Vec<_>>(),
        "tasks": catalog.tasks.iter().map(|task| json!({
            "name": task.name,
            "description": task.description,
            "input_schema": task.input_schema,
            "output_schema": task.output_schema,
        })).collect::<Vec<_>>(),
        "elicitation": catalog.elicitation.iter().map(|elicitation| json!({
            "name": elicitation.name,
            "description": elicitation.description,
            "schema": elicitation.schema,
        })).collect::<Vec<_>>(),
    })
}

fn render_provider_docs(snapshot: &Value) -> Result<String> {
    let mut out = String::from("# Generated Provider Surfaces\n\n");
    out.push_str("Generated by `cargo xtask generate-provider-surfaces`. Do not edit by hand.\n\n");
    out.push_str(&format!(
        "- Provider fingerprint: `{}`\n",
        snapshot["provider_fingerprint"].as_str().unwrap_or("")
    ));
    out.push_str("- Palette surface: deferred until the Axon tauri-palette port lands.\n\n");
    out.push_str("## Providers\n\n");
    for provider in snapshot["providers"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "### `{}` ({})\n\n",
            provider["name"].as_str().unwrap_or("unknown"),
            provider["kind"].as_str().unwrap_or("unknown")
        ));
        if let Some(description) = provider["description"].as_str() {
            out.push_str(description);
            out.push_str("\n\n");
        }
        out.push_str("| tool | MCP | CLI | REST | purpose |\n|---|---:|---:|---:|---|\n");
        for tool in provider["tools"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                tool["name"].as_str().unwrap_or(""),
                yes_no(tool["mcp"].as_bool().unwrap_or(false)),
                yes_no(tool["cli"].as_bool().unwrap_or(false)),
                yes_no(tool["rest"].as_bool().unwrap_or(false)),
                tool["description"]
                    .as_str()
                    .unwrap_or("")
                    .replace('|', "\\|"),
            ));
        }
        out.push('\n');
    }
    Ok(out)
}

fn render_provider_skill(provider: &Value) -> Result<String> {
    let name = provider["name"].as_str().unwrap_or("provider");
    let description = provider["description"]
        .as_str()
        .unwrap_or("Generated provider skill.");
    let action_names = provider["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str())
        .take(4)
        .collect::<Vec<_>>()
        .join(", ");
    let when_to_use = provider["when_to_use"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if action_names.is_empty() {
                format!("Use when working with the `{name}` provider.")
            } else {
                format!("Use when working with `{name}` provider actions such as {action_names}.")
            }
        });
    let mut out = format!(
        "---\nname: {name}\ndescription: {}\n---\n\nGenerated by `cargo xtask generate-provider-surfaces` from the current provider catalog.\n\n# `{name}` Provider\n\n{description}\n\n## When To Use\n\n{when_to_use}\n\n## Surface Selection\n\n",
        yaml_string(&when_to_use),
    );
    out.push_str(
        "- Use MCP first when the server is connected, especially for MCP-only actions.\n",
    );
    out.push_str("- Use CLI only when the action's CLI surface is `yes`; do not invent commands for `N/A` entries.\n");
    out.push_str("- Use REST only when the action's REST surface is `yes`; send JSON bodies matching the action schema.\n");
    out.push_str("- MCP-only elicitation actions require an elicitation-capable MCP client; do not attempt CLI or REST fallbacks.\n\n");
    out.push_str("## Tools\n\n");
    out.push_str("| tool | MCP | CLI | REST | CLI command | REST route | purpose |\n");
    out.push_str("|---|---:|---:|---:|---|---|---|\n");
    for tool in provider["tools"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | `{}` | `{}` | {} |\n",
            tool["name"].as_str().unwrap_or(""),
            yes_no(tool["mcp"].as_bool().unwrap_or(false)),
            yes_no(tool["cli"].as_bool().unwrap_or(false)),
            yes_no(tool["rest"].as_bool().unwrap_or(false)),
            tool["cli_command"].as_str().unwrap_or(""),
            tool["rest_route"].as_str().unwrap_or(""),
            tool["description"]
                .as_str()
                .unwrap_or("")
                .replace('|', "\\|"),
        ));
    }
    out.push_str("\n## Action Reference\n\n");
    for tool in provider["tools"].as_array().into_iter().flatten() {
        render_tool_reference(&mut out, tool);
    }
    render_primitive_section(&mut out, "Prompts", &provider["prompts"]);
    render_primitive_section(&mut out, "Resources", &provider["resources"]);
    render_primitive_section(&mut out, "Tasks", &provider["tasks"]);
    render_primitive_section(&mut out, "Elicitation", &provider["elicitation"]);
    Ok(out)
}

fn render_tool_reference(out: &mut String, tool: &Value) {
    let name = tool["name"].as_str().unwrap_or("");
    out.push_str(&format!("### `{name}`\n\n"));
    out.push_str(tool["description"].as_str().unwrap_or(""));
    out.push_str("\n\n");
    out.push_str(&format!(
        "- Scope: `{}`\n",
        tool["scope"].as_str().unwrap_or("public/default")
    ));
    out.push_str(&format!(
        "- Cost: `{}`\n",
        tool["cost"].as_str().unwrap_or("unspecified")
    ));
    out.push_str(&format!(
        "- Destructive: `{}`\n",
        tool["destructive"].as_bool().unwrap_or(false)
    ));
    out.push_str(&format!(
        "- Requires admin: `{}`\n",
        tool["requires_admin"].as_bool().unwrap_or(false)
    ));
    out.push_str(&format!(
        "- Required args: `{}`\n",
        schema_required_args(&tool["input_schema"])
    ));
    out.push_str(&format!(
        "- Optional args: `{}`\n",
        schema_optional_args(&tool["input_schema"])
    ));
    out.push_str(&format!("- Output: `{}`\n", output_summary(tool)));
    out.push_str(&format!("- MCP: `soma(action=\"{name}\")`\n"));
    if tool["cli"].as_bool().unwrap_or(false) {
        out.push_str(&format!(
            "- CLI: `soma {}`\n",
            cli_usage(tool)
                .unwrap_or_else(|| tool["cli_command"].as_str().unwrap_or(name).to_owned())
        ));
        if let Some(flags) = cli_flags_summary(tool) {
            out.push_str(&format!("- CLI flags: {flags}\n"));
        }
        if let Some(aliases) = tool["cli_aliases"].as_array() {
            let aliases = aliases
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            if !aliases.is_empty() {
                out.push_str(&format!("- CLI aliases: `{aliases}`\n"));
            }
        }
    } else {
        out.push_str("- CLI: `N/A` - do not call this action from CLI.\n");
    }
    if tool["rest"].as_bool().unwrap_or(false) {
        out.push_str(&format!(
            "- REST: `{}`\n",
            tool["rest_route"].as_str().unwrap_or("")
        ));
    } else {
        out.push_str("- REST: `N/A` - do not invent an HTTP route.\n");
    }
    if let Some(env) = env_summary(tool) {
        out.push_str(&format!("- Env: {env}\n"));
    }
    if let Some(examples) = examples_summary(tool) {
        out.push_str(&format!("- Examples: {examples}\n"));
    }
    if let Some(fallback) = scaffold_fallback_summary(tool) {
        out.push_str(&fallback);
    }
    out.push('\n');
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

fn schema_required_args(schema: &Value) -> String {
    let Some(required) = schema["required"].as_array() else {
        return "none".to_owned();
    };
    let args = required
        .iter()
        .filter_map(Value::as_str)
        .map(|name| format!("{name}: {}", schema_property_type(schema, name)))
        .collect::<Vec<_>>();
    if args.is_empty() {
        "none".to_owned()
    } else {
        args.join(", ")
    }
}

fn schema_optional_args(schema: &Value) -> String {
    let required = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let Some(properties) = schema["properties"].as_object() else {
        return "none".to_owned();
    };
    let args = properties
        .keys()
        .filter(|name| !required.contains(name.as_str()))
        .map(|name| format!("{name}: {}", schema_property_type(schema, name)))
        .collect::<Vec<_>>();
    if args.is_empty() {
        "none".to_owned()
    } else {
        args.join(", ")
    }
}

fn schema_property_type(schema: &Value, name: &str) -> String {
    let property = &schema["properties"][name];
    property["type"]
        .as_str()
        .or_else(|| property["format"].as_str())
        .or_else(|| property["$ref"].as_str())
        .unwrap_or("value")
        .to_owned()
}

fn output_summary(tool: &Value) -> String {
    if let Some(returns) = tool["meta"]["returns"].as_str() {
        return returns.to_owned();
    }
    schema_output_summary(&tool["output_schema"])
}

fn schema_output_summary(schema: &Value) -> String {
    if schema.is_null() {
        return "unspecified".to_owned();
    }
    if let Some(properties) = schema["properties"].as_object() {
        let fields = properties
            .keys()
            .take(6)
            .map(|name| format!("{name}: {}", schema_property_type(schema, name)))
            .collect::<Vec<_>>();
        if !fields.is_empty() {
            return fields.join(", ");
        }
    }
    schema["type"]
        .as_str()
        .or_else(|| schema["$ref"].as_str())
        .unwrap_or("structured JSON")
        .to_owned()
}

fn cli_usage(tool: &Value) -> Option<String> {
    tool["cli_usage"]
        .as_str()
        .map(|usage| usage.strip_prefix("soma ").unwrap_or(usage).to_owned())
}

fn cli_flags_summary(tool: &Value) -> Option<String> {
    let flags = tool["cli_flags"].as_array()?;
    let rendered = flags
        .iter()
        .filter_map(|flag| {
            let name = flag["name"].as_str()?;
            let value_name = flag["value_name"]
                .as_str()
                .map(|value| format!(" {value}"))
                .unwrap_or_default();
            let required = if flag["required"].as_bool().unwrap_or(false) {
                " required"
            } else {
                " optional"
            };
            Some(format!("`{name}{value_name}`{required}"))
        })
        .collect::<Vec<_>>();
    (!rendered.is_empty()).then(|| rendered.join(", "))
}

fn scaffold_fallback_summary(tool: &Value) -> Option<String> {
    let fallback = &tool["meta"]["scaffold_fallback"];
    if fallback.is_null() {
        return None;
    }
    let skill = fallback["recommended_skill"].as_str()?;
    let instructions = fallback["instructions"].as_str().unwrap_or("");
    Some(format!(
        "- Elicitation fallback: recommended_skill: `{skill}`. {instructions}\n"
    ))
}

fn env_summary(tool: &Value) -> Option<String> {
    let env = tool["env"].as_array()?;
    let entries = env
        .iter()
        .filter_map(|item| {
            let name = item["name"].as_str()?;
            let mut qualifiers = Vec::new();
            if item["required"].as_bool().unwrap_or(false) {
                qualifiers.push("required");
            }
            if item["sensitive"].as_bool().unwrap_or(false) {
                qualifiers.push("sensitive");
            }
            if qualifiers.is_empty() {
                Some(format!("`{name}`"))
            } else {
                Some(format!("`{name}` ({})", qualifiers.join(", ")))
            }
        })
        .collect::<Vec<_>>();
    (!entries.is_empty()).then(|| entries.join(", "))
}

fn examples_summary(tool: &Value) -> Option<String> {
    let examples = tool["examples"].as_array()?;
    let rendered = examples
        .iter()
        .take(2)
        .map(|example| {
            let name = example["name"].as_str().unwrap_or("soma");
            if let Some(args) = example["args"].as_object() {
                let args = args
                    .keys()
                    .map(|key| format!("`{key}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("`{name}` args: {args}")
            } else {
                format!("`{name}`")
            }
        })
        .collect::<Vec<_>>();
    (!rendered.is_empty()).then(|| rendered.join("; "))
}

fn render_primitive_section(out: &mut String, title: &str, items: &Value) {
    let Some(items) = items.as_array() else {
        return;
    };
    if items.is_empty() {
        return;
    }
    out.push_str(&format!("## {title}\n\n"));
    for item in items {
        let name = item["name"].as_str().unwrap_or("");
        let description = item["description"].as_str().unwrap_or("");
        out.push_str(&format!("- `{name}`: {description}\n"));
        if let Some(uri) = item["uri_template"].as_str() {
            out.push_str(&format!("  - URI template: `{uri}`\n"));
        }
    }
    out.push('\n');
}

fn render_distribution_plugin(snapshot: &Value) -> Value {
    json!({
        "schema_version": 1,
        "name": "soma",
        "description": "Generated distributable plugin surface for Soma.",
        "provider_fingerprint": snapshot["provider_fingerprint"].clone(),
        "plugin_root": "plugins/soma",
        "codex": {
            "plugin_json": "plugins/soma/.codex-plugin/plugin.json",
            "marketplace": ".agents/plugins/marketplace.json"
        },
        "claude": {
            "plugin_json": "plugins/soma/.claude-plugin/plugin.json",
            "marketplace": ".claude-plugin/marketplace.json"
        },
        "skills": "plugins/soma/skills",
        "node_package": "packages/soma-rmcp/package.json",
        "docs": "docs/generated/provider-surfaces.md",
        "mcp_server": "server.json",
        "provider_files": snapshot["surfaces"]["provider_files"].clone(),
        "surfaces": snapshot["surfaces"].clone(),
        "providers": snapshot["providers"].clone()
    })
}

fn generated_skill_paths(catalogs: &[ProviderCatalog]) -> Vec<String> {
    let mut paths = catalogs
        .iter()
        .map(|catalog| format!("docs/generated/skills/{}/SKILL.md", catalog.provider.name))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn write_or_check_generated_skills(root: &Path, snapshot: &Value, mode: Mode) -> Result<()> {
    let skills_root = root.join("docs/generated/skills");
    for provider in snapshot["providers"].as_array().into_iter().flatten() {
        let name = provider["name"].as_str().unwrap_or("provider");
        let path = skills_root.join(name).join("SKILL.md");
        let content = render_provider_skill(provider)?;
        if mode.should_write() {
            write_if_changed(&path, &content)?;
            println!("wrote {}", relative_display(root, &path));
        }
        if mode.should_check() {
            if !path.exists() {
                bail!(
                    "{} is missing; run cargo xtask generate-provider-surfaces --write",
                    relative_display(root, &path)
                );
            }
            let current = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            if current != content {
                bail!(
                    "{} is stale; run cargo xtask generate-provider-surfaces --write",
                    relative_display(root, &path)
                );
            }
        }
    }
    Ok(())
}

fn render_codex_marketplace() -> Value {
    json!({
        "name": "soma",
        "plugins": [{
            "name": "soma",
            "source": {
                "source": "local",
                "path": "./plugins/soma"
            },
            "policy": {
                "installation": "AVAILABLE",
                "authentication": "ON_INSTALL"
            },
            "category": "Infrastructure",
            "interface": {
                "displayName": "Soma"
            }
        }]
    })
}

fn render_claude_marketplace() -> Value {
    json!({
        "$schema": "https://json.schemastore.org/claude-code-marketplace.json",
        "name": "soma",
        "description": "Generated marketplace catalog for Soma plugins.",
        "owner": {
            "name": "Jacob Magar",
            "email": "jmagar@users.noreply.github.com"
        },
        "plugins": [{
            "name": "soma",
            "description": "Soma RMCP runtime plugin.",
            "source": "./plugins/soma",
            "category": "infrastructure"
        }]
    })
}

#[derive(Debug, Clone, Copy)]
enum Surface {
    Mcp,
    Cli,
}

fn surface_actions(catalogs: &[ProviderCatalog], surface: Surface) -> Vec<String> {
    let mut actions = catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .filter_map(|tool| {
            let enabled = match surface {
                Surface::Mcp => tool.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true),
                Surface::Cli => tool.cli.as_ref().map(|cli| cli.enabled).unwrap_or(false),
            };
            enabled.then(|| tool.name.clone())
        })
        .collect::<Vec<_>>();
    actions.sort();
    actions
}

fn rest_routes(catalogs: &[ProviderCatalog]) -> Vec<String> {
    let mut routes = catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .filter_map(|tool| {
            let rest = tool.rest.as_ref()?;
            rest.enabled.then(|| {
                format!(
                    "{} {}",
                    rest.method.as_deref().unwrap_or("POST"),
                    rest.path
                        .clone()
                        .unwrap_or_else(|| format!("/v1/{}", tool.name))
                )
            })
        })
        .collect::<Vec<_>>();
    routes.sort();
    routes
}

fn cli_commands(catalogs: &[ProviderCatalog]) -> Vec<String> {
    let mut commands = catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .flat_map(|tool| {
            let Some(cli) = &tool.cli else {
                return Vec::new();
            };
            if !cli.enabled {
                return Vec::new();
            }
            let mut commands = vec![cli.command.clone().unwrap_or_else(|| tool.name.clone())];
            commands.extend(cli.aliases.clone());
            commands
        })
        .collect::<Vec<_>>();
    commands.sort();
    commands
}

fn provider_dir() -> std::path::PathBuf {
    std::env::var_os("SOMA_PROVIDER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("providers"))
}

fn provider_files(provider_dir: &Path) -> Result<Vec<String>> {
    if !provider_dir.exists() {
        return Ok(Vec::new());
    }
    let label = provider_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("providers");
    let mut files = fs::read_dir(provider_dir)
        .with_context(|| format!("failed to read {}", provider_dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("json" | "ts" | "wasm" | "py")
            )
        })
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("{label}/{name}"))
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn canonical_json(value: &Value) -> Result<String> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if path.exists()
        && fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?
            == content
    {
        return Ok(());
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn mixed_drop_in_providers_populate_generated_distribution_surfaces() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp = tempfile::tempdir().expect("tempdir");
        let providers = temp.path().join("providers");
        fs::create_dir(&providers).expect("providers dir");
        fs::write(
            providers.join("weather.tool.ts"),
            format!(
                "export default {};\nexport async function call(input) {{ return {{ ok: true, action: input.action }}; }}\n",
                provider_manifest("weather-ts", "ai-sdk", "weather_ts")
            ),
        )
        .expect("ts provider");
        fs::write(
            providers.join("image.wasm"),
            wasm_provider(provider_manifest("image-wasm", "wasm", "image_wasm").as_bytes()),
        )
        .expect("wasm provider");
        fs::write(
            providers.join("python_math.py"),
            r#"
PROVIDER = {"name": "python-math", "kind": "python"}

def python_add(a: int, b: int) -> int:
    """Add two integers."""
    return a + b
"#,
        )
        .expect("python provider");
        fs::write(
            providers.join("notes.mcp.json"),
            provider_manifest("notes-mcp", "mcp", "notes_search"),
        )
        .expect("mcp provider");
        fs::write(
            providers.join("github.openapi.json"),
            provider_manifest("github-openapi", "openapi", "github_issue"),
        )
        .expect("openapi provider");

        std::env::set_var("SOMA_PROVIDER_DIR", &providers);
        let snapshot = render_provider_snapshot().expect("snapshot");
        std::env::remove_var("SOMA_PROVIDER_DIR");
        let plugin = render_distribution_plugin(&snapshot);

        for action in [
            "weather_ts",
            "image_wasm",
            "python_add",
            "notes_search",
            "github_issue",
        ] {
            assert!(
                contains_string(&snapshot["surfaces"]["mcp_actions"], action),
                "MCP actions should include {action}"
            );
            assert!(
                contains_string(&snapshot["surfaces"]["cli_actions"], action),
                "CLI actions should include {action}"
            );
        }
        assert!(contains_string(
            &snapshot["surfaces"]["cli_commands"],
            "ship-weather-ts"
        ));
        assert!(contains_string(
            &snapshot["surfaces"]["cli_commands"],
            "ship-alias-weather_ts"
        ));
        assert!(contains_string(
            &snapshot["surfaces"]["rest_routes"],
            "POST /v1/providers/weather-ts"
        ));
        assert!(contains_string(
            &plugin["provider_files"],
            "providers/weather.tool.ts"
        ));
        assert!(contains_string(
            &plugin["provider_files"],
            "providers/python_math.py"
        ));
        assert!(contains_string(
            &plugin["provider_files"],
            "providers/github.openapi.json"
        ));
        assert!(contains_string(
            &snapshot["surfaces"]["generated_skills"],
            "docs/generated/skills/weather-ts/SKILL.md"
        ));
        let skill = render_provider_skill(
            snapshot["providers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|provider| provider["name"] == "weather-ts")
                .unwrap(),
        )
        .expect("skill");
        assert!(skill.contains("name: weather-ts"));
        assert!(skill.contains("When To Use"));
        assert!(skill.contains("weather_ts"));
        assert!(skill.contains("ship-weather-ts"));
        assert!(skill.contains("POST /v1/providers/weather-ts"));
        assert!(skill.contains("MCP"));
        assert!(skill.contains("CLI"));
        assert!(skill.contains("REST"));
        assert!(skill.contains("## Action Reference"));
        assert!(skill.contains("Required args"));
        assert!(skill.contains("Output"));

        let static_skill = render_provider_skill(
            snapshot["providers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|provider| provider["name"] == "static-rust")
                .unwrap(),
        )
        .expect("static skill");
        assert!(static_skill.contains("| `elicit_name` | yes | no | no | `N/A` | `N/A` |"));
        assert!(static_skill.contains("- CLI: `N/A` - do not call this action from CLI."));
        assert!(static_skill.contains("- REST: `N/A` - do not invent an HTTP route."));
        assert!(static_skill.contains("Generated by `cargo xtask generate-provider-surfaces`"));
        assert!(static_skill.contains("Soma built-in Rust actions"));
        assert!(static_skill.contains("MCP elicitation"));
        assert!(static_skill.contains("- CLI: `soma echo --message MSG`"));
        assert!(static_skill.contains("- CLI flags: `--message MSG` required"));
        assert!(static_skill.contains("- Output: `EchoResult`"));
        assert!(static_skill.contains("- Output: `ScaffoldIntentReport`"));
        assert!(static_skill.contains("recommended_skill: `scaffold-project`"));
        assert!(static_skill.contains("Do not mutate files until the user approves the plan."));
    }

    fn provider_manifest(name: &str, kind: &str, action: &str) -> String {
        json!({
            "schema_version": 1,
            "provider": {
                "name": name,
                "kind": kind,
                "enabled": true,
                "description": format!("Generated test provider {name}.")
            },
            "tools": [{
                "name": action,
                "description": format!("Generated test action {action}."),
                "input_schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {}
                },
                "cli": {
                    "enabled": true,
                    "command": format!("ship-{name}"),
                    "aliases": [format!("ship-alias-{action}")]
                },
                "rest": {
                    "enabled": true,
                    "method": "POST",
                    "path": format!("/v1/providers/{name}")
                }
            }]
        })
        .to_string()
    }

    fn wasm_provider(manifest: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0, b'a', b's', b'm', 1, 0, 0, 0];
        let name = b"soma.provider";
        let mut payload = Vec::new();
        write_leb(name.len() as u32, &mut payload);
        payload.extend_from_slice(name);
        payload.extend_from_slice(manifest);
        bytes.push(0);
        write_leb(payload.len() as u32, &mut bytes);
        bytes.extend(payload);
        bytes
    }

    fn write_leb(mut value: u32, bytes: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    fn contains_string(value: &Value, needle: &str) -> bool {
        value
            .as_array()
            .into_iter()
            .flatten()
            .any(|value| value.as_str() == Some(needle))
    }
}
