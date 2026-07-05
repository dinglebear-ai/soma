use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{cargo_generate, cargo_generate_post, command_exists};

const RESERVED_RMCP_PORTS: &[u16] = &[40010, 40020, 40030, 40040, 40050, 40060, 40070, 40080];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerCategory {
    UpstreamClient,
    ApplicationPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortSelection {
    Auto,
    Port(u16),
}

#[derive(Debug)]
pub(crate) struct ScaffoldCliInput {
    pub name: String,
    pub category: ServerCategory,
    pub port: PortSelection,
    pub github_owner: String,
    pub github_repo: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ScaffoldPlan {
    pub defines: BTreeMap<String, String>,
    pub report: String,
    action_snippets: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IntentPayload {
    server_category: String,
    project: IntentProject,
    #[serde(default)]
    runtime: IntentRuntime,
    #[serde(default)]
    upstream: IntentUpstream,
    #[serde(default)]
    required_surfaces: Vec<String>,
    #[serde(default)]
    mcp_primitives: Vec<String>,
    #[serde(default)]
    deployment: String,
    #[serde(default)]
    plugins: Vec<String>,
    #[serde(default)]
    publish_mcp: bool,
    #[serde(default)]
    crawl_docs: CrawlDocs,
}

#[derive(Debug, Deserialize)]
struct IntentProject {
    #[serde(default)]
    display_name: String,
    crate_name: String,
    binary_name: String,
    service_name: String,
    env_prefix: String,
}

#[derive(Debug, Default, Deserialize)]
struct IntentRuntime {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    binary_profile: String,
    #[serde(default = "default_mcp_transport")]
    mcp_transport: String,
}

#[derive(Debug, Default, Deserialize)]
struct IntentUpstream {
    #[serde(default)]
    auth_kind: String,
}

#[derive(Debug, Default, Deserialize)]
struct CrawlDocs {
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    repos: Vec<String>,
    #[serde(default)]
    search_topics: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActionManifest {
    actions: Vec<ActionDraft>,
}

#[derive(Debug, Deserialize)]
struct ActionDraft {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_read_scope")]
    scope: String,
    #[serde(default)]
    params: Vec<ActionParam>,
}

#[derive(Debug, Deserialize)]
struct ActionParam {
    name: String,
    #[serde(rename = "type", default = "default_string_type")]
    ty: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    description: String,
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    let options = Options::parse(args)?;
    match options.mode {
        Mode::Help => {
            print_help();
            Ok(())
        }
        Mode::Verify { root } => {
            verify_generated_project(&root)?;
            if options.cargo_check {
                run_cargo_check(&root)?;
            }
            println!("Scaffold verification passed for {}", root.display());
            Ok(())
        }
        Mode::AdaptPlan { ref root } => {
            print!("{}", render_adapt_plan(root)?);
            Ok(())
        }
        Mode::WriteActionStarters { ref root } => {
            let actions = options
                .actions
                .as_ref()
                .context("--actions is required with --write-action-starters")?;
            let text = fs::read_to_string(actions)
                .with_context(|| format!("failed to read action manifest {}", actions.display()))?;
            let manifest = ActionManifest::from_json(&text)?;
            write_action_starters(root, &manifest)?;
            println!(
                "Wrote action starter artifacts to {}",
                root.join("docs/action-starters").display()
            );
            Ok(())
        }
        Mode::Plan => {
            let plan = build_plan(&options)?;
            print!("{}", plan.render());
            Ok(())
        }
        Mode::Apply { ref parent } => {
            let plan = build_plan(&options)?;
            apply_plan(&plan, parent, options.cargo_check)?;
            Ok(())
        }
    }
}

impl ScaffoldPlan {
    pub(crate) fn from_intent_json(input: &str) -> Result<Self> {
        let payload: IntentPayload =
            serde_json::from_str(input).context("failed to parse scaffold intent JSON")?;
        Self::from_intent(payload)
    }

    pub(crate) fn from_cli(input: ScaffoldCliInput) -> Result<Self> {
        let raw_name = normalize_name(&input.name)?;
        let crate_prefix = raw_name
            .strip_suffix("-mcp")
            .unwrap_or(&raw_name)
            .to_owned();
        let service_slug = snake_slug(&crate_prefix)?;
        let package_name = if raw_name.ends_with("-mcp") {
            raw_name
        } else {
            format!("{raw_name}-mcp")
        };
        let default_port = match input.port {
            PortSelection::Auto => next_scaffold_port(40010, RESERVED_RMCP_PORTS).to_string(),
            PortSelection::Port(port) => validate_port(port)?.to_string(),
        };
        let github_repo = input.github_repo.unwrap_or_else(|| package_name.clone());

        let mut defines = BTreeMap::new();
        defines.insert("package_name".to_owned(), package_name.clone());
        defines.insert("crate_prefix".to_owned(), crate_prefix.clone());
        defines.insert("binary_name".to_owned(), crate_prefix.clone());
        defines.insert(
            "server_binary_name".to_owned(),
            format!("{crate_prefix}-server"),
        );
        defines.insert("service_slug".to_owned(), service_slug.clone());
        defines.insert("type_prefix".to_owned(), pascal_case(&service_slug));
        defines.insert("env_prefix".to_owned(), env_prefix(&service_slug));
        defines.insert("scope_prefix".to_owned(), crate_prefix.replace('_', "-"));
        defines.insert("default_port".to_owned(), default_port);
        defines.insert("github_owner".to_owned(), input.github_owner);
        defines.insert("github_repo".to_owned(), github_repo);
        defines.insert(
            "default_features".to_owned(),
            default_features_for(input.category).to_owned(),
        );

        let empty_surfaces = Vec::new();
        let empty_primitives = Vec::new();
        let empty_plugins = Vec::new();
        let empty_crawl_docs = CrawlDocs::default();
        let report = render_report(ReportInput {
            category: input.category,
            defines: &defines,
            required_surfaces: &empty_surfaces,
            host: "127.0.0.1",
            mcp_transport: "dual",
            mcp_primitives: &empty_primitives,
            deployment: "none",
            plugins: &empty_plugins,
            publish_mcp: false,
            crawl_docs: &empty_crawl_docs,
            display_name: "",
            auth_kind: "",
        });
        Ok(Self {
            defines,
            report,
            action_snippets: None,
        })
    }

    fn from_intent(payload: IntentPayload) -> Result<Self> {
        let category = parse_category(&payload.server_category)?;
        let crate_prefix = payload
            .project
            .crate_name
            .strip_suffix("-mcp")
            .unwrap_or(&payload.project.crate_name)
            .to_owned();
        let service_slug = snake_slug(&payload.project.service_name)?;
        let binary_profile = normalize_binary_profile(&payload.runtime.binary_profile, category);
        let default_port = validate_port(if payload.runtime.port == 0 {
            next_scaffold_port(40010, RESERVED_RMCP_PORTS)
        } else {
            payload.runtime.port
        })?
        .to_string();

        let mut defines = BTreeMap::new();
        defines.insert(
            "package_name".to_owned(),
            payload.project.crate_name.clone(),
        );
        defines.insert("crate_prefix".to_owned(), crate_prefix.clone());
        defines.insert(
            "binary_name".to_owned(),
            payload.project.binary_name.clone(),
        );
        defines.insert(
            "server_binary_name".to_owned(),
            format!("{}-server", payload.project.binary_name),
        );
        defines.insert("service_slug".to_owned(), service_slug.clone());
        defines.insert("type_prefix".to_owned(), pascal_case(&service_slug));
        defines.insert("env_prefix".to_owned(), payload.project.env_prefix.clone());
        defines.insert("scope_prefix".to_owned(), crate_prefix.replace('_', "-"));
        defines.insert("default_port".to_owned(), default_port);
        defines.insert("github_owner".to_owned(), "jmagar".to_owned());
        defines.insert("github_repo".to_owned(), payload.project.crate_name.clone());
        defines.insert("default_features".to_owned(), binary_profile);

        let report = render_report(ReportInput {
            category,
            defines: &defines,
            required_surfaces: &payload.required_surfaces,
            host: &payload.runtime.host,
            mcp_transport: &payload.runtime.mcp_transport,
            mcp_primitives: &payload.mcp_primitives,
            deployment: &payload.deployment,
            plugins: &payload.plugins,
            publish_mcp: payload.publish_mcp,
            crawl_docs: &payload.crawl_docs,
            display_name: &payload.project.display_name,
            auth_kind: &payload.upstream.auth_kind,
        });
        Ok(Self {
            defines,
            report,
            action_snippets: None,
        })
    }

    fn with_action_snippets(mut self, snippets: Option<String>) -> Self {
        self.action_snippets = snippets;
        self
    }

    fn render(&self) -> String {
        let mut output = self.report.clone();
        if let Some(snippets) = &self.action_snippets {
            output.push_str("\n## Action Starter Snippets\n\n");
            output.push_str(snippets);
            output.push('\n');
        }
        output
    }
}

impl ActionManifest {
    pub(crate) fn from_json(input: &str) -> Result<Self> {
        let manifest: Self =
            serde_json::from_str(input).context("failed to parse action manifest JSON")?;
        if manifest.actions.is_empty() {
            bail!("action manifest must contain at least one action");
        }
        for action in &manifest.actions {
            validate_identifier("action.name", &action.name)?;
            for param in &action.params {
                validate_identifier("param.name", &param.name)?;
            }
        }
        Ok(manifest)
    }

    pub(crate) fn render_snippets(&self, service_type: &str) -> String {
        let mut output = String::new();
        output.push_str("### crates/rtemplate-service/src/actions.rs\n\n```rust\n");
        output.push_str(&self.render_action_specs_snippet());
        output.push_str("```\n\n### crates/rtemplate-mcp/src/tools.rs\n\n```rust\n");
        output.push_str(&self.render_tools_snippet());
        output.push_str("```\n\n### crates/rtemplate-cli/src/lib.rs\n\n```rust\n");
        output.push_str(&self.render_cli_snippet());
        output.push_str("```\n\n### crates/rtemplate-service/src/app.rs\n\n```rust\n");
        output.push_str(&self.render_service_snippet(service_type));
        output.push_str("```\n\n### tests\n\n");
        output.push_str(&self.render_tests_guide());
        output
    }

    fn render_action_specs_snippet(&self) -> String {
        let mut output = String::new();
        for action in &self.actions {
            output.push_str("ActionSpec {\n");
            output.push_str(&format!("    name: \"{}\",\n", action.name));
            output.push_str(&format!(
                "    description: \"{}\",\n",
                escape_rust_string(description_or_default(&action.description, &action.name))
            ));
            output.push_str(&format!(
                "    required_scope: Some({}_SCOPE),\n",
                action.scope.to_ascii_uppercase()
            ));
            output.push_str("    transport: ActionTransport::Any,\n");
            output.push_str("    rest_method: Some(\"POST\"),\n");
            output.push_str(&format!("    rest_path: Some(\"/v1/{}\"),\n", action.name));
            output.push_str("    destructive: false,\n");
            output.push_str("    requires_admin: false,\n");
            output.push_str("    cost: ActionCost::Cheap,\n");
            output.push_str("    params: &[\n");
            for param in &action.params {
                output.push_str(&format!(
                    "        ParamSpec {{ name: \"{}\", ty: ParamType::String, required: {}, description: \"{}\", max_len: Some(4096), enum_values: &[] }},\n",
                    param.name,
                    param.required,
                    escape_rust_string(description_or_default(&param.description, &param.name))
                ));
            }
            output.push_str("    ],\n");
            output.push_str("    returns: \"JSON payload from the service layer.\",\n");
            output.push_str("    cli: None,\n");
            output.push_str("},\n\n");
        }
        output
    }

    fn render_tools_snippet(&self) -> String {
        let mut output = String::new();
        for action in &self.actions {
            output.push_str(&format!("\"{}\" => {{\n", action.name));
            for param in &action.params {
                if param.ty == "string" {
                    output.push_str(&format!(
                        "    let {} = string_arg(&args, \"{}\");\n",
                        param.name, param.name
                    ));
                }
            }
            output.push_str(&format!("    state.service.{}(", action.name));
            output.push_str(
                &action
                    .params
                    .iter()
                    .map(|param| param.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            output.push_str(").await\n},\n");
        }
        output
    }

    fn render_cli_snippet(&self) -> String {
        let mut output = String::new();
        for action in &self.actions {
            output.push_str(&format!("Command::{},\n", pascal_case(&action.name)));
        }
        output
    }

    fn render_service_snippet(&self, service_type: &str) -> String {
        let mut output = String::new();
        output.push_str(&format!("impl {service_type} {{\n"));
        for action in &self.actions {
            output.push_str(&format!("    pub async fn {}(", action.name));
            output.push_str(
                &action
                    .params
                    .iter()
                    .map(|param| format!("{}: Option<String>", param.name))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            output.push_str(") -> anyhow::Result<serde_json::Value> {\n");
            output.push_str("        todo!(\"replace with service implementation\")\n");
            output.push_str("    }\n\n");
        }
        output.push_str("}\n");
        output
    }

    fn render_tests_guide(&self) -> String {
        let mut output = String::new();
        output.push_str("Add service and tool_dispatch coverage for every action.\n");
        output.push_str(
            "- `crates/rmcp-template/tests/tool_dispatch.rs`: MCP success and validation paths.\n",
        );
        output.push_str("- `crates/rmcp-template/tests/cli_parse.rs`: CLI command/flag parsing.\n");
        output.push_str("- Service-layer tests near `crates/rtemplate-service/src/app.rs` or focused modules.\n");
        output.push_str(&format!(
            "- Actions: {}\n",
            self.actions
                .iter()
                .map(|action| action.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        output
    }

    fn render_starter_readme(&self) -> String {
        let mut output = String::new();
        output.push_str("# Action Starters\n\n");
        output.push_str("Generated by `cargo xtask scaffold --write-action-starters`.\n");
        output.push_str("Review and move these snippets into the real source files; they are intentionally not applied automatically.\n\n");
        output.push_str("Actions:\n");
        for action in &self.actions {
            output.push_str(&format!("- `{}` - {}\n", action.name, action.description));
        }
        output
    }
}

fn build_plan(options: &Options) -> Result<ScaffoldPlan> {
    let plan = if let Some(path) = &options.intent {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read intent JSON {}", path.display()))?;
        ScaffoldPlan::from_intent_json(&text)?
    } else {
        let name = options
            .name
            .clone()
            .context("--name is required when --intent is not provided")?;
        ScaffoldPlan::from_cli(ScaffoldCliInput {
            name,
            category: options.category.unwrap_or(ServerCategory::UpstreamClient),
            port: options.port.unwrap_or(PortSelection::Auto),
            github_owner: options
                .github_owner
                .clone()
                .unwrap_or_else(|| "jmagar".to_owned()),
            github_repo: options.github_repo.clone(),
        })?
    };

    let action_snippets = options
        .actions
        .as_ref()
        .map(|path| -> Result<String> {
            let text = fs::read_to_string(path)
                .with_context(|| format!("failed to read action manifest {}", path.display()))?;
            Ok(ActionManifest::from_json(&text)?.render_snippets("ExampleService"))
        })
        .transpose()?;
    Ok(plan.with_action_snippets(action_snippets))
}

fn apply_plan(plan: &ScaffoldPlan, parent: &Path, cargo_check: bool) -> Result<()> {
    if !command_exists("cargo-generate") {
        bail!("cargo-generate is not installed; run `cargo install cargo-generate`");
    }
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create output parent {}", parent.display()))?;
    let package_name = plan
        .defines
        .get("package_name")
        .context("scaffold plan is missing package_name")?;
    let repo = std::env::current_dir().context("failed to read current directory")?;
    let temp = cargo_generate::TempDir::new("rtemplate-scaffold")?;
    let template = temp.path().join("_template");
    cargo_generate::stage_template(&repo, &template)?;
    let mut args = vec![
        "generate".to_owned(),
        "--silent".to_owned(),
        "--path".to_owned(),
        template.display().to_string(),
        "--name".to_owned(),
        package_name.clone(),
        "--destination".to_owned(),
        parent.display().to_string(),
    ];
    for (key, value) in &plan.defines {
        args.push("--define".to_owned());
        args.push(format!("{key}={value}"));
    }
    run_command("cargo", &args, Path::new("."))?;

    let project = parent.join(package_name);
    cargo_generate_post::run(&[project.display().to_string()])?;

    fs::create_dir_all(project.join("docs"))
        .with_context(|| format!("failed to create {}", project.join("docs").display()))?;
    fs::write(project.join("docs/scaffold-report.md"), plan.render())
        .with_context(|| "failed to write scaffold report".to_owned())?;
    verify_generated_project(&project)?;
    if cargo_check {
        run_cargo_check(&project)?;
    }
    println!("Scaffolded {}", project.display());
    println!(
        "Report: {}",
        project.join("docs/scaffold-report.md").display()
    );
    Ok(())
}

pub(crate) fn verify_generated_project(root: &Path) -> Result<()> {
    let mut errors = Vec::new();
    for relative in [
        ".cargo-generate-values.toml",
        "cargo-generate.toml",
        "template",
        "docs/CARGO_GENERATE.md",
    ] {
        if root.join(relative).exists() {
            errors.push(format!("generated project still contains {relative}"));
        }
    }
    if root.join("CLAUDE.md").exists() {
        check_agent_symlink(root, "AGENTS.md", &mut errors);
        check_agent_symlink(root, "GEMINI.md", &mut errors);
    }
    check_plugin_manifest_versions(root, &mut errors)?;
    if errors.is_empty() {
        Ok(())
    } else {
        bail!(errors.join("\n"))
    }
}

fn check_plugin_manifest_versions(root: &Path, errors: &mut Vec<String>) -> Result<()> {
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let normalized = path.to_string_lossy().replace('\\', "/");
        if !(normalized.ends_with(".claude-plugin/plugin.json")
            || normalized.ends_with(".codex-plugin/plugin.json")
            || normalized.ends_with("gemini-extension.json"))
        {
            continue;
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if json_contains_key(&value, "version") {
            errors.push(format!(
                "{} must not contain a version field",
                path.display()
            ));
        }
    }
    Ok(())
}

fn check_agent_symlink(root: &Path, name: &str, errors: &mut Vec<String>) {
    let path = root.join(name);
    match fs::read_link(&path) {
        Ok(target) if target == Path::new("CLAUDE.md") => {}
        Ok(target) => errors.push(format!(
            "{} must be a symlink to CLAUDE.md, found {}",
            path.display(),
            target.display()
        )),
        Err(_) => errors.push(format!("{} must be a symlink to CLAUDE.md", path.display())),
    }
}

fn json_contains_key(value: &serde_json::Value, key: &str) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key(key) || map.values().any(|value| json_contains_key(value, key))
        }
        serde_json::Value::Array(values) => {
            values.iter().any(|value| json_contains_key(value, key))
        }
        _ => false,
    }
}

fn run_cargo_check(root: &Path) -> Result<()> {
    run_command(
        "cargo",
        &[
            "check".to_owned(),
            "--workspace".to_owned(),
            "--all-targets".to_owned(),
        ],
        root,
    )
}

fn run_command(program: &str, args: &[String], cwd: &Path) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd).stdin(Stdio::null());
    if program == "cargo" {
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("CARGO_PROFILE") {
                command.env_remove(key);
            }
        }
    }
    let status = command
        .status()
        .with_context(|| format!("failed to spawn `{program}` in {}", cwd.display()))?;
    if !status.success() {
        bail!(
            "`{program} {}` exited with status {status} in {}",
            args.join(" "),
            cwd.display()
        );
    }
    Ok(())
}

struct ReportInput<'a> {
    category: ServerCategory,
    defines: &'a BTreeMap<String, String>,
    required_surfaces: &'a [String],
    host: &'a str,
    mcp_transport: &'a str,
    mcp_primitives: &'a [String],
    deployment: &'a str,
    plugins: &'a [String],
    publish_mcp: bool,
    crawl_docs: &'a CrawlDocs,
    display_name: &'a str,
    auth_kind: &'a str,
}

fn render_report(input: ReportInput<'_>) -> String {
    let surfaces = if input.required_surfaces.is_empty() {
        surfaces_for(input.category)
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    } else {
        input.required_surfaces.to_vec()
    };
    let title = if input.display_name.is_empty() {
        input
            .defines
            .get("package_name")
            .map(String::as_str)
            .unwrap_or("Generated MCP server")
    } else {
        input.display_name
    };
    let mut report = String::new();
    report.push_str(&format!("# Scaffold Plan: {title}\n\n"));
    report.push_str(&format!("Category: {}\n", input.category.as_str()));
    report.push_str(&format!("Required surfaces: {}\n", surfaces.join(", ")));
    report.push_str(&format!(
        "Default Cargo features: {}\n",
        input
            .defines
            .get("default_features")
            .map(String::as_str)
            .unwrap_or("")
    ));
    report.push_str(&format!("Host: {}\n", input.host));
    report.push_str(&format!("MCP transport: {}\n", input.mcp_transport));
    report.push_str(&format!(
        "MCP primitives: {}\n",
        join_or_none(input.mcp_primitives)
    ));
    report.push_str(&format!(
        "Deployment: {}\n",
        empty_as_none(input.deployment)
    ));
    report.push_str(&format!("Plugins: {}\n", join_or_none(input.plugins)));
    report.push_str(&format!("MCP registry publishing: {}\n", input.publish_mcp));
    report.push_str(&format!(
        "Upstream auth: {}\n\n",
        empty_as_none(input.auth_kind)
    ));
    report.push_str("## cargo-generate Defines\n\n");
    for (key, value) in input.defines {
        report.push_str(&format!("- `{key}` = `{value}`\n"));
    }
    report.push_str("\n## One-command Paths\n\n");
    report.push_str("- Plan only: `cargo xtask scaffold --intent scaffold-intent.json --plan`\n");
    report.push_str(
        "- Apply: `cargo xtask scaffold --intent scaffold-intent.json --apply ../generated`\n",
    );
    report.push_str(
        "- Prove generated output: `cargo xtask scaffold --verify ../generated/<package>`\n",
    );
    if has_crawl_docs(input.crawl_docs) {
        report.push_str("\n## Axon research inputs\n\n");
        for url in &input.crawl_docs.urls {
            report.push_str(&format!("- URL: {url}\n"));
        }
        for repo in &input.crawl_docs.repos {
            report.push_str(&format!("- Repo: {repo}\n"));
        }
        for topic in &input.crawl_docs.search_topics {
            report.push_str(&format!("- Search: {topic}\n"));
        }
        report.push_str(
            "\nRun the approved Axon crawl/research step before replacing stub client methods.\n",
        );
    }
    report.push_str("\n## Remaining Human Work\n\n");
    report.push_str("- Replace the stub client with real upstream/platform calls.\n");
    report.push_str("- Add each real action through service, MCP, CLI, tests, and docs.\n");
    report.push_str("- Run `cargo xtask scaffold --verify <generated-root>` before publishing.\n");
    report
}

fn has_crawl_docs(crawl_docs: &CrawlDocs) -> bool {
    !crawl_docs.urls.is_empty()
        || !crawl_docs.repos.is_empty()
        || !crawl_docs.search_topics.is_empty()
}

fn surfaces_for(category: ServerCategory) -> &'static [&'static str] {
    match category {
        ServerCategory::UpstreamClient => &["mcp", "cli"],
        ServerCategory::ApplicationPlatform => &["api", "cli", "mcp", "web"],
    }
}

fn default_features_for(category: ServerCategory) -> &'static str {
    match category {
        ServerCategory::UpstreamClient => "local-adapter",
        ServerCategory::ApplicationPlatform => "full",
    }
}

fn normalize_binary_profile(value: &str, category: ServerCategory) -> String {
    match value {
        "local-adapter" | "cli-mcp" => "local-adapter".to_owned(),
        "server-full" | "full" => "full".to_owned(),
        "" => default_features_for(category).to_owned(),
        other => other.to_owned(),
    }
}

fn parse_category(value: &str) -> Result<ServerCategory> {
    match value {
        "upstream-client" => Ok(ServerCategory::UpstreamClient),
        "application-platform" => Ok(ServerCategory::ApplicationPlatform),
        other => bail!("unknown server category {other:?}"),
    }
}

impl ServerCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::UpstreamClient => "upstream-client",
            Self::ApplicationPlatform => "application-platform",
        }
    }
}

fn next_scaffold_port(start: u16, reserved: &[u16]) -> u16 {
    let reserved = reserved.iter().copied().collect::<BTreeSet<_>>();
    let mut port = start;
    loop {
        if !reserved.contains(&port) {
            return port;
        }
        port = port.saturating_add(10);
    }
}

fn normalize_name(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    validate_slug("name", &normalized)?;
    Ok(normalized)
}

fn snake_slug(value: &str) -> Result<String> {
    let slug = value.replace('-', "_");
    validate_identifier("service_slug", &slug)?;
    Ok(slug)
}

fn pascal_case(value: &str) -> String {
    value
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn env_prefix(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn validate_slug(name: &str, value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{name} must not be empty");
    };
    if !first.is_ascii_lowercase()
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        bail!("{name} must match ^[a-z][a-z0-9-]*$: {value:?}");
    }
    Ok(())
}

fn validate_identifier(name: &str, value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{name} must not be empty");
    };
    if !first.is_ascii_lowercase()
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        bail!("{name} must be Rust identifier-safe: {value:?}");
    }
    Ok(())
}

fn validate_port(port: u16) -> Result<u16> {
    if port == 0 {
        bail!("port must be between 1 and 65535");
    }
    Ok(port)
}

fn description_or_default<'a>(description: &'a str, name: &'a str) -> &'a str {
    if description.is_empty() {
        name
    } else {
        description
    }
}

fn escape_rust_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(", ")
    }
}

fn empty_as_none(value: &str) -> &str {
    if value.is_empty() {
        "none"
    } else {
        value
    }
}

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_mcp_transport() -> String {
    "dual".to_owned()
}

fn default_read_scope() -> String {
    "read".to_owned()
}

fn default_string_type() -> String {
    "string".to_owned()
}

#[derive(Debug)]
struct Options {
    mode: Mode,
    intent: Option<PathBuf>,
    name: Option<String>,
    category: Option<ServerCategory>,
    port: Option<PortSelection>,
    github_owner: Option<String>,
    github_repo: Option<String>,
    actions: Option<PathBuf>,
    cargo_check: bool,
}

#[derive(Debug)]
enum Mode {
    Help,
    Plan,
    Apply { parent: PathBuf },
    Verify { root: PathBuf },
    AdaptPlan { root: PathBuf },
    WriteActionStarters { root: PathBuf },
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        if args
            .iter()
            .any(|arg| arg == "--help" || arg == "-h" || arg == "help")
        {
            return Ok(Self::help());
        }
        let mut options = Self {
            mode: Mode::Plan,
            intent: None,
            name: None,
            category: None,
            port: None,
            github_owner: None,
            github_repo: None,
            actions: None,
            cargo_check: true,
        };
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--plan" => options.mode = Mode::Plan,
                "--apply" => {
                    index += 1;
                    options.mode = Mode::Apply {
                        parent: PathBuf::from(value_arg(args, index, "--apply")?),
                    };
                }
                "--verify" => {
                    index += 1;
                    options.mode = Mode::Verify {
                        root: PathBuf::from(value_arg(args, index, "--verify")?),
                    };
                }
                "--adapt-plan" => {
                    index += 1;
                    options.mode = Mode::AdaptPlan {
                        root: PathBuf::from(value_arg(args, index, "--adapt-plan")?),
                    };
                }
                "--write-action-starters" => {
                    index += 1;
                    options.mode = Mode::WriteActionStarters {
                        root: PathBuf::from(value_arg(args, index, "--write-action-starters")?),
                    };
                }
                "--intent" => {
                    index += 1;
                    options.intent = Some(PathBuf::from(value_arg(args, index, "--intent")?));
                }
                "--name" => {
                    index += 1;
                    options.name = Some(value_arg(args, index, "--name")?.to_owned());
                }
                "--category" => {
                    index += 1;
                    options.category = Some(parse_category(value_arg(args, index, "--category")?)?);
                }
                "--port" => {
                    index += 1;
                    options.port = Some(parse_port_selection(value_arg(args, index, "--port")?)?);
                }
                "--github-owner" => {
                    index += 1;
                    options.github_owner =
                        Some(value_arg(args, index, "--github-owner")?.to_owned());
                }
                "--github-repo" => {
                    index += 1;
                    options.github_repo = Some(value_arg(args, index, "--github-repo")?.to_owned());
                }
                "--actions" => {
                    index += 1;
                    options.actions = Some(PathBuf::from(value_arg(args, index, "--actions")?));
                }
                "--no-cargo-check" => options.cargo_check = false,
                unknown => bail!("unknown scaffold option {unknown:?}"),
            }
            index += 1;
        }
        Ok(options)
    }

    fn help() -> Self {
        Self {
            mode: Mode::Help,
            intent: None,
            name: None,
            category: None,
            port: None,
            github_owner: None,
            github_repo: None,
            actions: None,
            cargo_check: true,
        }
    }
}

fn value_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .with_context(|| format!("{flag} requires a value"))
}

fn parse_port_selection(value: &str) -> Result<PortSelection> {
    if value == "auto" {
        return Ok(PortSelection::Auto);
    }
    let port = value
        .parse::<u16>()
        .with_context(|| format!("port must be an integer or auto: {value:?}"))?;
    Ok(PortSelection::Port(validate_port(port)?))
}

fn print_help() {
    println!(
        "Usage:
  cargo xtask scaffold --name <service> [--category upstream-client|application-platform] [--port auto|PORT] --plan
  cargo xtask scaffold --intent scaffold-intent.json [--actions actions.json] --plan
  cargo xtask scaffold --intent scaffold-intent.json --apply <output-parent> [--no-cargo-check]
  cargo xtask scaffold --verify <generated-root> [--no-cargo-check]
  cargo xtask scaffold --adapt-plan <generated-root>
  cargo xtask scaffold --write-action-starters <generated-root> --actions actions.json"
    );
}

fn write_action_starters(root: &Path, manifest: &ActionManifest) -> Result<()> {
    if !root.exists() {
        bail!("generated root does not exist: {}", root.display());
    }
    let dir = root.join("docs/action-starters");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    fs::write(dir.join("README.md"), manifest.render_starter_readme())
        .context("failed to write action starter README")?;
    fs::write(
        dir.join("actions.rs.snippet"),
        manifest.render_action_specs_snippet(),
    )
    .context("failed to write actions.rs snippet")?;
    fs::write(
        dir.join("tools.rs.snippet"),
        manifest.render_tools_snippet(),
    )
    .context("failed to write tools.rs snippet")?;
    fs::write(dir.join("cli.rs.snippet"), manifest.render_cli_snippet())
        .context("failed to write cli.rs snippet")?;
    fs::write(
        dir.join("service.rs.snippet"),
        manifest.render_service_snippet("ExampleService"),
    )
    .context("failed to write service.rs snippet")?;
    fs::write(dir.join("tests.md"), manifest.render_tests_guide())
        .context("failed to write tests guide")?;
    Ok(())
}

fn render_adapt_plan(root: &Path) -> Result<String> {
    if !root.exists() {
        bail!("generated root does not exist: {}", root.display());
    }
    let report_path = root.join("docs/scaffold-report.md");
    let report = fs::read_to_string(&report_path).unwrap_or_default();
    let profile = report_value(&report, "Default Cargo features").unwrap_or("unknown");
    let category = report_value(&report, "Category").unwrap_or("unknown");
    let surfaces = report_value(&report, "Required surfaces").unwrap_or("unknown");
    let has_api = profile_contains(profile, "full")
        || profile_contains(profile, "server")
        || surface_contains(surfaces, "api");
    let has_web = profile_contains(profile, "full") || surface_contains(surfaces, "web");
    let has_plugin = profile_contains(profile, "full") || profile_contains(profile, "plugin");

    let mut output = String::new();
    output.push_str("# Adaptation Plan\n\n");
    output.push_str(&format!("Root: {}\n", root.display()));
    output.push_str(&format!("Category: {category}\n"));
    output.push_str(&format!("Profile: {profile}\n"));
    output.push_str(&format!("Surfaces: {surfaces}\n"));
    if report.is_empty() {
        output.push_str(&format!(
            "Scaffold report: missing ({})\n",
            report_path.display()
        ));
    } else {
        output.push_str(&format!("Scaffold report: {}\n", report_path.display()));
    }

    output.push_str("\n## 1. Domain and config\n\n");
    output.push_str("- Replace the stub client in `crates/rtemplate-service/src/example.rs`.\n");
    output.push_str("- Put validation, defaults, retries, caching, and domain rules in `crates/rtemplate-service/src/app.rs` or focused modules under `crates/rtemplate-service/src/`.\n");
    output.push_str(
        "- Update config structs and env prefixes in `crates/rtemplate-contracts/src/config.rs`.\n",
    );
    output.push_str("- Update `.env.example` and `config.example.toml` with real required credentials and non-secret defaults.\n");

    output.push_str("\n## 2. Business actions\n\n");
    output.push_str(
        "- Add action metadata and dispatch in `crates/rtemplate-service/src/actions.rs`.\n",
    );
    output.push_str("- Regenerate MCP schema docs and OpenAPI after changing action metadata.\n");
    output.push_str("- Keep MCP, CLI, and REST shims registry-driven.\n");
    if has_api {
        output.push_str("- Add REST handlers/routes in `crates/rtemplate-api/src/api.rs` and `crates/rmcp-template/src/routes.rs`.\n");
    } else {
        output.push_str("- REST handlers are optional for this profile; add them only if the project needs an API surface.\n");
    }

    output.push_str("\n## 3. Optional surfaces\n\n");
    if has_web {
        output.push_str("- Replace or remove the bundled web app under `apps/web`.\n");
        output.push_str("- Run `cargo xtask sync-web-source` after web source changes.\n");
    } else {
        output.push_str("- Web is not selected by this profile; remove web-specific assumptions if you keep the scaffold lean.\n");
    }
    if has_plugin {
        output.push_str("- Update plugin options, skills, and setup mappings under `plugins/rtemplate/` and `crates/rtemplate-cli/src/setup.rs`.\n");
    } else {
        output.push_str("- Plugin support is not selected by this profile; keep plugin files only if you plan to publish editor integrations.\n");
    }
    output.push_str("- Update `server.json`, repository URLs, Docker labels, release metadata, and package names before publishing.\n");

    output.push_str("\n## 4. Tests and verification\n\n");
    output.push_str("- Add service behavior tests near the service modules.\n");
    output.push_str(
        "- Add MCP dispatch coverage in `crates/rmcp-template/tests/tool_dispatch.rs`.\n",
    );
    output.push_str("- Add CLI parsing coverage in `crates/rmcp-template/tests/cli_parse.rs`.\n");
    if has_api {
        output.push_str("- Add REST route coverage for every API action.\n");
    }
    output.push_str("- Run `cargo xtask scaffold --verify <generated-root>`.\n");
    output.push_str("- Run `cargo xtask check-docs`, `cargo xtask check-schema-docs --check`, `cargo xtask check-openapi --check`, and `just verify`.\n");
    output.push_str("\nUse this plan as an implementation checklist; it does not mutate files.\n");
    Ok(output)
}

fn report_value<'a>(report: &'a str, label: &str) -> Option<&'a str> {
    report.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() == label {
            Some(value.trim())
        } else {
            None
        }
    })
}

fn profile_contains(profile: &str, needle: &str) -> bool {
    profile.split(',').map(str::trim).any(|part| part == needle)
}

fn surface_contains(surfaces: &str, needle: &str) -> bool {
    surfaces
        .split(',')
        .map(str::trim)
        .any(|part| part == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn intent_json_derives_local_adapter_defines_and_research_plan() {
        let intent = r#"{
          "kind": "rmcp_template_scaffold_intent",
          "schema_version": 1,
          "server_category": "upstream-client",
          "required_surfaces": ["mcp", "cli"],
          "project": {
            "display_name": "Unraid MCP",
            "crate_name": "unraid-mcp",
            "binary_name": "runraid",
            "service_name": "unraid",
            "env_prefix": "UNRAID"
          },
          "upstream": { "base_url_env": "UNRAID_API_URL", "auth_kind": "api-key" },
          "runtime": {
            "host": "127.0.0.1",
            "port": 40010,
            "binary_profile": "local-adapter",
            "mcp_transport": "dual"
          },
          "mcp_primitives": ["tools", "resources"],
          "deployment": "none",
          "plugins": ["claude", "codex"],
          "publish_mcp": true,
          "crawl_docs": {
            "urls": ["https://docs.unraid.net/"],
            "repos": [],
            "search_topics": ["Unraid API authentication"]
          },
          "handoff": { "recommended_skill": "scaffold-project", "instructions": "Plan only." },
          "policy": {
            "business_action_minimum_surfaces": ["mcp", "cli"],
            "upstream_client_surfaces": ["mcp", "cli"],
            "application_platform_surfaces": ["api", "cli", "mcp", "web"],
            "binary_profiles": {
              "upstream_client_default": "local-adapter",
              "application_platform_default": "server-full",
              "gateway_shared_default": "server-full"
            }
          }
        }"#;

        let plan = ScaffoldPlan::from_intent_json(intent).expect("plan");

        assert_eq!(plan.defines.get("package_name").unwrap(), "unraid-mcp");
        assert_eq!(plan.defines.get("crate_prefix").unwrap(), "unraid");
        assert_eq!(plan.defines.get("binary_name").unwrap(), "runraid");
        assert_eq!(
            plan.defines.get("server_binary_name").unwrap(),
            "runraid-server"
        );
        assert_eq!(
            plan.defines.get("default_features").unwrap(),
            "local-adapter"
        );
        assert!(plan.report.contains("Required surfaces: mcp, cli"));
        assert!(plan.report.contains("Axon research inputs"));
        assert!(plan.report.contains("https://docs.unraid.net/"));
    }

    #[test]
    fn cli_name_auto_port_derives_defaults() {
        let plan = ScaffoldPlan::from_cli(ScaffoldCliInput {
            name: "rustfoo".to_owned(),
            category: ServerCategory::UpstreamClient,
            port: PortSelection::Auto,
            github_owner: "jmagar".to_owned(),
            github_repo: None,
        })
        .expect("plan");

        assert_eq!(plan.defines.get("package_name").unwrap(), "rustfoo-mcp");
        assert_eq!(plan.defines.get("crate_prefix").unwrap(), "rustfoo");
        assert_eq!(plan.defines.get("type_prefix").unwrap(), "Rustfoo");
        assert_eq!(plan.defines.get("env_prefix").unwrap(), "RUSTFOO");
        assert_eq!(
            plan.defines.get("default_features").unwrap(),
            "local-adapter"
        );
        assert_eq!(plan.defines.get("default_port").unwrap(), "40090");
    }

    #[test]
    fn action_manifest_renders_starter_snippets() {
        let manifest = r#"{
          "actions": [
            {
              "name": "list_things",
              "description": "List visible things.",
              "scope": "read",
              "params": [
                { "name": "kind", "type": "string", "required": false }
              ]
            }
          ]
        }"#;

        let snippets = ActionManifest::from_json(manifest)
            .expect("manifest")
            .render_snippets("ExampleService");

        assert!(snippets.contains("ActionSpec"));
        assert!(snippets.contains("name: \"list_things\""));
        assert!(snippets.contains("string_arg(&args, \"kind\")"));
        assert!(snippets.contains("Command::ListThings"));
        assert!(snippets.contains("tool_dispatch"));
    }

    #[test]
    fn generated_project_verify_rejects_plugin_manifest_versions() {
        let fixture = TempDir::new().unwrap();
        fs::write(fixture.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(fixture.path().join("CLAUDE.md"), "# Memory\n").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("CLAUDE.md", fixture.path().join("AGENTS.md")).unwrap();
            std::os::unix::fs::symlink("CLAUDE.md", fixture.path().join("GEMINI.md")).unwrap();
        }
        fs::create_dir_all(fixture.path().join("plugins/example/.codex-plugin")).unwrap();
        fs::write(
            fixture
                .path()
                .join("plugins/example/.codex-plugin/plugin.json"),
            r#"{"name":"example","version":"1.0.0"}"#,
        )
        .unwrap();

        let errors = verify_generated_project(fixture.path()).expect_err("version rejected");
        assert!(errors
            .to_string()
            .contains("must not contain a version field"));
    }

    #[test]
    fn adapt_plan_is_profile_aware_and_path_specific() {
        let fixture = TempDir::new().unwrap();
        fs::create_dir_all(fixture.path().join("docs")).unwrap();
        fs::write(
            fixture.path().join("docs/scaffold-report.md"),
            "Category: application-platform\nRequired surfaces: api, cli, mcp, web\nDefault Cargo features: full\n",
        )
        .unwrap();

        let plan = render_adapt_plan(fixture.path()).expect("adapt plan");

        assert!(plan.contains("# Adaptation Plan"));
        assert!(plan.contains("Profile: full"));
        assert!(plan.contains("crates/rtemplate-service/src/example.rs"));
        assert!(plan.contains("crates/rtemplate-api/src/api.rs"));
        assert!(plan.contains("apps/web"));
        assert!(plan.contains("server.json"));
        assert!(plan.contains("cargo xtask scaffold --verify"));
    }

    #[test]
    fn action_manifest_writes_starter_artifacts_into_generated_project() {
        let fixture = TempDir::new().unwrap();
        let manifest = ActionManifest::from_json(
            r#"{
              "actions": [
                {
                  "name": "list_things",
                  "description": "List visible things.",
                  "scope": "read",
                  "params": [
                    { "name": "kind", "type": "string", "required": false }
                  ]
                }
              ]
            }"#,
        )
        .expect("manifest");

        write_action_starters(fixture.path(), &manifest).expect("write starters");

        let readme = fs::read_to_string(fixture.path().join("docs/action-starters/README.md"))
            .expect("readme");
        let actions = fs::read_to_string(
            fixture
                .path()
                .join("docs/action-starters/actions.rs.snippet"),
        )
        .expect("actions snippet");
        let service = fs::read_to_string(
            fixture
                .path()
                .join("docs/action-starters/service.rs.snippet"),
        )
        .expect("service snippet");
        let tests = fs::read_to_string(fixture.path().join("docs/action-starters/tests.md"))
            .expect("tests guide");

        assert!(readme.contains("list_things"));
        assert!(actions.contains("ActionSpec"));
        assert!(actions.contains("name: \"list_things\""));
        assert!(service.contains("pub async fn list_things"));
        assert!(tests.contains("tool_dispatch"));
    }
}
