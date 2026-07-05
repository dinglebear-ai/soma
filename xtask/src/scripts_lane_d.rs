//! Lane D Rust migrations for Python scripts that still have compatibility
//! wrappers in `scripts/`.
//!
//! Intended parent wiring:
//! - `asciicheck` maps to `scripts/asciicheck.py`
//! - `check_openapi` maps to `scripts/check-openapi.py`
//! - `check_schema_docs` maps to `scripts/check-schema-docs.py`
//! - `check_scaffold_intent_contract` maps to
//!   `scripts/check-scaffold-intent-contract.py`
//! - `check_cargo_generate` delegates to the existing xtask cargo-generate
//!   implementation; the Python file is already only a compatibility wrapper.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const REST_SCHEMAS: &[(&str, Option<&str>, &str)] = &[
    ("greet", Some("GreetRequest"), "GreetResponse"),
    ("echo", Some("EchoRequest"), "EchoResponse"),
    ("status", None, "StatusResponse"),
    ("help", None, "HelpResponse"),
];

const SURFACES: &[&str] = &["api", "cli", "mcp", "web"];
const AUTH_KINDS: &[&str] = &["api-key", "bearer", "both", "none", "oauth", "other"];
const TRANSPORTS: &[&str] = &["dual", "http", "stdio"];
const BINARY_PROFILES: &[&str] = &["local-adapter", "server-full"];
const PRIMITIVES: &[&str] = &["elicitation", "prompts", "resources", "tools"];
const DEPLOYMENTS: &[&str] = &["docker", "none", "systemd"];
const PLUGINS: &[&str] = &["claude", "codex", "gemini"];

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionEntry {
    name: String,
    description: String,
    scope: String,
    doc_scope: String,
    transport: String,
    rest_method: Option<String>,
    rest_path: Option<String>,
    cost: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckMode {
    Check,
    Write,
    CheckAndWrite,
}

impl CheckMode {
    fn parse(args: &[String], usage: &str) -> Result<Self> {
        let mut write = false;
        let mut check = false;
        for arg in args {
            match arg.as_str() {
                "--write" => write = true,
                "--check" => check = true,
                "--help" | "-h" => {
                    println!("{usage}");
                    return Ok(Self::Check);
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

pub fn asciicheck(args: &[String]) -> Result<()> {
    let (fix, files) = parse_asciicheck_args(args)?;
    let mut has_errors = false;
    for file in files {
        has_errors |= lint_utf8_ascii(Path::new(file), fix)?;
    }
    if has_errors {
        bail!("ASCII check failed");
    }
    Ok(())
}

pub fn check_openapi(args: &[String]) -> Result<()> {
    let mode = CheckMode::parse(args, "Usage: cargo xtask check-openapi [--write] [--check]")?;
    let root = current_dir()?;
    let rendered_value = render_openapi(&root)?;
    let rendered = canonical_json(&rendered_value)?;
    let out = root.join("docs/generated/openapi.json");
    let mut failures = validate_openapi(&root, &rendered_value)?;

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
            failures.push(
                "docs/generated/openapi.json is missing; run scripts/check-openapi.py --write"
                    .to_owned(),
            );
        } else if read(&out)? != rendered {
            failures.push(
                "docs/generated/openapi.json is stale; run scripts/check-openapi.py --write"
                    .to_owned(),
            );
        }
    }

    finish_failures(failures)?;
    if mode.should_check() {
        println!("OpenAPI schema is current");
    }
    Ok(())
}

pub fn check_schema_docs(args: &[String]) -> Result<()> {
    let mode = CheckMode::parse(
        args,
        "Usage: cargo xtask check-schema-docs [--write] [--check]",
    )?;
    let root = current_dir()?;
    let doc = root.join("docs/MCP_SCHEMA.md");
    let rendered = render_schema_docs(&root)?;

    if mode.should_write() {
        fs::write(&doc, &rendered).with_context(|| format!("failed to write {}", doc.display()))?;
        println!("wrote {}", relative_display(&root, &doc));
    }

    let mut failures = Vec::new();
    if mode.should_check() {
        if !doc.exists() {
            failures.push("docs/MCP_SCHEMA.md is missing; run --write".to_owned());
        } else if read(&doc)? != rendered {
            failures.push("docs/MCP_SCHEMA.md is stale; run --write".to_owned());
        }
        let actions = extract_actions(&root)?;
        failures.extend(check_schema_mentions(&root, &actions)?);
        failures.extend(check_schema_scope(&root, &actions)?);
    }

    finish_failures(failures)?;
    if mode.should_check() {
        println!("schema docs are current");
    }
    Ok(())
}

pub fn check_scaffold_intent_contract() -> Result<()> {
    let root = current_dir()?;
    let schema = root.join("docs/contracts/scaffold-intent.schema.json");
    let examples = root.join("docs/contracts/examples");

    validate_scaffold_schema(&schema)?;
    let mut found = false;
    for entry in
        fs::read_dir(&examples).with_context(|| format!("failed to read {}", examples.display()))?
    {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("scaffold-intent-") && name.ends_with(".json") {
            found = true;
            let payload = load_json(&path)?;
            validate_scaffold_payload(&payload, &path)?;
        }
    }
    require(
        found,
        format!("{}: no scaffold intent examples found", examples.display()),
    )?;
    println!("scaffold intent contract and examples are valid");
    Ok(())
}

pub fn check_cargo_generate(args: &[String]) -> Result<()> {
    crate::cargo_generate::run(args)
}

fn parse_asciicheck_args(args: &[String]) -> Result<(bool, Vec<&str>)> {
    let mut fix = false;
    let mut files = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--fix" => fix = true,
            "--help" | "-h" => {
                println!("Usage: cargo xtask asciicheck [--fix] <files>...");
                return Ok((false, Vec::new()));
            }
            file => files.push(file),
        }
    }
    if files.is_empty() {
        bail!("the following required arguments were not provided: files");
    }
    Ok((fix, files))
}

fn lint_utf8_ascii(filename: &Path, fix: bool) -> Result<bool> {
    let raw =
        fs::read(filename).with_context(|| format!("failed to read {}", filename.display()))?;
    let text = match String::from_utf8(raw.clone()) {
        Ok(text) => text,
        Err(exc) => {
            let offset = exc.utf8_error().valid_up_to();
            let partial = &raw[..offset];
            let line = partial.iter().filter(|byte| **byte == b'\n').count() + 1;
            let col = offset
                - partial
                    .iter()
                    .rposition(|byte| *byte == b'\n')
                    .map(|index| index + 1)
                    .unwrap_or(0)
                + 1;
            println!("{}: UTF-8 decoding error:", filename.display());
            println!("  byte offset: {offset}");
            println!("  reason: {}", exc.utf8_error());
            println!("  location: line {line}, column {col}");
            return Ok(true);
        }
    };

    let mut errors = Vec::new();
    for (line_index, line) in text.split_inclusive('\n').enumerate() {
        for (col_index, ch) in line.chars().enumerate() {
            let codepoint = ch as u32;
            if matches!(ch, '\n' | '\r' | '\t') {
                continue;
            }
            if !(0x20..=0x7e).contains(&codepoint) && !allowed_unicode(codepoint) {
                errors.push((line_index + 1, col_index + 1, ch, codepoint));
            }
        }
    }

    if !errors.is_empty() {
        println!("{}:", filename.display());
        for (lineno, colno, ch, codepoint) in &errors {
            println!(
                "  line {lineno}, column {colno}: U+{codepoint:04X} ({})",
                escape_char(*ch)
            );
        }
    }

    if !errors.is_empty() && fix {
        let mut replacements = 0usize;
        let new_contents: String = text
            .chars()
            .flat_map(|ch| {
                if let Some(replacement) = substitution(ch as u32) {
                    replacements += 1;
                    replacement.chars().collect::<Vec<_>>()
                } else {
                    vec![ch]
                }
            })
            .collect();
        fs::write(filename, new_contents)
            .with_context(|| format!("failed to write {}", filename.display()))?;
        println!("  fixed {replacements} replaceable character(s)");
    }

    Ok(!errors.is_empty())
}

fn allowed_unicode(codepoint: u32) -> bool {
    matches!(
        codepoint,
        0x00A7
            | 0x00D7
            | 0x2013
            | 0x2014
            | 0x2026
            | 0x20AC
            | 0x2190
            | 0x2192
            | 0x2193
            | 0x2194
            | 0x2248
            | 0x2264
            | 0x2265
            | 0x2500
            | 0x2501
            | 0x2502
            | 0x250C
            | 0x2510
            | 0x2514
            | 0x2518
            | 0x251C
            | 0x26A0
            | 0x2713
            | 0x2717
            | 0xFE0F
    )
}

fn substitution(codepoint: u32) -> Option<&'static str> {
    match codepoint {
        0x00A0 | 0x202F => Some(" "),
        0x2011 | 0x2013 | 0x2014 => Some("-"),
        0x2018 | 0x2019 => Some("'"),
        0x201C | 0x201D => Some("\""),
        0x2026 => Some("..."),
        _ => None,
    }
}

fn escape_char(ch: char) -> String {
    match ch {
        '\n' => "\\n".to_owned(),
        '\r' => "\\r".to_owned(),
        '\t' => "\\t".to_owned(),
        '\'' => "\\'".to_owned(),
        '"' => "\\\"".to_owned(),
        '\\' => "\\\\".to_owned(),
        other => other.to_string(),
    }
}

fn render_openapi(root: &Path) -> Result<Value> {
    let entries = action_entries(root)?;
    let rest_actions: Vec<&ActionEntry> = entries
        .iter()
        .filter(|entry| {
            entry.transport == "Any" && entry.rest_method.is_some() && entry.rest_path.is_some()
        })
        .collect();
    let action_names: Vec<String> = rest_actions
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    let version = package_version(root)?;
    let port = default_mcp_port(root)?;

    let mut paths = serde_json::Map::new();
    paths.insert(
        "/health".to_owned(),
        json!({"get":{"tags":["health"],"summary":"Liveness probe","operationId":"getHealth","security":[],"responses":{"200":{"description":"Server is alive","content":{"application/json":{"schema":schema_ref("HealthResponse"),"examples":{"ok":{"value":{"status":"ok"}}}}}}}}}),
    );
    paths.insert(
        "/openapi.json".to_owned(),
        json!({"get":{"tags":["health"],"summary":"OpenAPI schema","operationId":"getOpenApiSchema","security":[],"responses":{"200":{"description":"Generated OpenAPI schema for the REST surface","content":{"application/json":{"schema":{"type":"object","additionalProperties":true}}}}}}}),
    );
    paths.insert(
        "/status".to_owned(),
        json!({"get":{"tags":["health"],"summary":"Local runtime status","operationId":"getLocalStatus","security":[],"responses":{"200":{"description":"Runtime status with secrets redacted","content":{"application/json":{"schema":schema_ref("StatusResponse")}}},"500":{"$ref":"#/components/responses/InternalError"}}}}),
    );
    paths.insert(
        "/v1/capabilities".to_owned(),
        json!({"get":{"tags":["capabilities"],"summary":"Direct REST route inventory","operationId":"getCapabilities","security":[{"BearerAuth":[]},{}],"responses":{"200":{"description":"Supported direct REST routes and metadata","content":{"application/json":{"schema":schema_ref("CapabilitiesResponse")}}},"401":{"$ref":"#/components/responses/Unauthorized"}}}}),
    );

    for action in &rest_actions {
        let Some((request_schema, response_schema)) = rest_schemas(&action.name) else {
            continue;
        };
        let method = action
            .rest_method
            .as_deref()
            .expect("rest actions are filtered to require method")
            .to_ascii_lowercase();
        let path = action
            .rest_path
            .as_deref()
            .expect("rest actions are filtered to require path");
        let mut operation = json!({
            "tags": ["direct-rest"],
            "summary": format!("Run {}", action.name),
            "description": "Direct REST route over the shared service layer.",
            "operationId": format!("{method}{}", title_no_underscore(&action.name)),
            "security": [{"BearerAuth":[]},{}],
            "responses": {
                "200": {"description": format!("{} result", action.name), "content": {"application/json": {"schema": schema_ref(response_schema)}}},
                "400": {"$ref":"#/components/responses/BadRequest"},
                "401": {"$ref":"#/components/responses/Unauthorized"},
                "403": {"$ref":"#/components/responses/Forbidden"},
                "500": {"$ref":"#/components/responses/InternalError"}
            }
        });
        if let Some(request_schema) = request_schema {
            operation["requestBody"] = json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": schema_ref(request_schema),
                        "examples": { action.name.clone(): { "value": param_example(&action.name) } }
                    }
                }
            });
        }
        paths.insert(path.to_owned(), json!({method: operation}));
    }

    let direct_rest_routes = rest_actions
        .iter()
        .map(|action| {
            (
                action.name.clone(),
                json!({
                    "method": action.rest_method.as_deref().unwrap_or_default().to_uppercase(),
                    "path": action.rest_path.as_deref().unwrap_or_default(),
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let action_costs = entries
        .iter()
        .map(|entry| (entry.name.clone(), json!(entry.cost)))
        .collect::<serde_json::Map<_, _>>();
    let mcp_only: Vec<String> = entries
        .iter()
        .filter(|entry| entry.transport == "McpOnly")
        .map(|entry| entry.name.clone())
        .collect();

    Ok(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Example MCP REST API",
            "version": version,
            "description": "Generated OpenAPI schema for rmcp-template's direct REST surface. TEMPLATE: rename Example identifiers and action schemas when adapting. Auth modes: loopback/trusted-gateway deployments may have no local auth; mounted bearer mode uses RTEMPLATE_MCP_TOKEN; OAuth mode uses bearer JWTs. REST actions require their action-specific scopes when auth is mounted."
        },
        "servers": [{"url": format!("http://localhost:{port}"),"description":"Default local development server"}],
        "tags": [
            {"name":"health","description":"Unauthenticated runtime probes"},
            {"name":"capabilities","description":"REST route inventory"},
            {"name":"direct-rest","description":"Typed REST routes"}
        ],
        "paths": paths,
        "components": {
            "securitySchemes": {"BearerAuth":{"type":"http","scheme":"bearer","bearerFormat":"opaque","description":"Static bearer token in bearer mode; OAuth mode also uses bearer JWTs. Loopback and trusted-gateway modes may not require local auth."}},
            "schemas": openapi_schemas(action_names.clone()),
            "responses": {
                "BadRequest":{"description":"Validation error","content":{"application/json":{"schema":schema_ref("ErrorResponse")}}},
                "Unauthorized":{"description":"Missing or invalid authentication","content":{"application/json":{"schema":schema_ref("ErrorResponse")}}},
                "Forbidden":{"description":"Authenticated request lacks the required scope","content":{"application/json":{"schema":schema_ref("ErrorResponse")}}},
                "InternalError":{"description":"Internal server error","content":{"application/json":{"schema":schema_ref("ErrorResponse")}}}
            }
        },
        "x-template": {
            "source": "scripts/check-openapi.py",
            "action_metadata": "crates/rtemplate-contracts/src/actions.rs",
            "preferred_rest_style": "direct_routes",
            "rest_actions": action_names,
            "direct_rest_routes": direct_rest_routes,
            "action_costs": action_costs,
            "mcp_only_actions": mcp_only
        }
    }))
}

fn openapi_schemas(action_names: Vec<String>) -> Value {
    json!({
        "ActionName":{"type":"string","enum":action_names,"description":"REST-capable action names from crates/rtemplate-contracts/src/actions.rs."},
        "GreetRequest":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string","description":"Name to greet. Omit to greet the world."}}},
        "EchoRequest":{"type":"object","additionalProperties":false,"required":["message"],"properties":{"message":{"type":"string","minLength":1,"description":"Message to echo back. Must not be empty."}}},
        "ActionResponse":{"oneOf":[schema_ref("GreetResponse"),schema_ref("EchoResponse"),schema_ref("StatusResponse"),schema_ref("HelpResponse"),schema_ref("RestTruncationResponse")]},
        "GreetResponse":{"type":"object","required":["greeting","target"],"properties":{"greeting":{"type":"string"},"target":{"type":"string"},"server":{"type":"string"}},"additionalProperties":true},
        "EchoResponse":{"type":"object","required":["echo"],"properties":{"echo":{"type":"string"}},"additionalProperties":true},
        "StatusResponse":{"type":"object","required":["status"],"properties":{"status":{"type":"string"},"note":{"type":"string"},"server":{"type":"string"},"version":{"type":"string"},"transport":{"type":"string"}},"additionalProperties":true},
        "HealthResponse":{"type":"object","required":["status"],"properties":{"status":{"type":"string","const":"ok"}},"additionalProperties":false},
        "CapabilitiesResponse":{"type":"object","required":["server","version","preferred_rest_style","supported_routes","routes"],"properties":{"server":{"type":"string"},"version":{"type":"string"},"preferred_rest_style":{"type":"string","const":"direct_routes"},"supported_routes":{"type":"array","items":{"type":"string"}},"routes":{"type":"array","items":schema_ref("RestRoute")}},"additionalProperties":false},
        "RestRoute":{"type":"object","required":["method","path","auth","description"],"properties":{"method":{"type":"string"},"path":{"type":"string"},"action":{"type":["string","null"]},"auth":{"type":"string"},"description":{"type":"string"}},"additionalProperties":false},
        "HelpResponse":{"type":"object","required":["actions","mcp_only_actions","usage","examples"],"properties":{"actions":{"type":"array","items":schema_ref("ActionName")},"mcp_only_actions":{"type":"array","items":{"type":"string"}},"usage":{"type":"string"},"examples":{"type":"object","additionalProperties":true}},"additionalProperties":true},
        "ErrorResponse":{"type":"object","required":["error"],"properties":{"error":{"type":"string"}},"additionalProperties":false},
        "RestTruncationResponse":{"type":"object","required":["truncated","error","max_response_bytes","hint"],"properties":{"truncated":{"type":"boolean","const":true},"error":{"type":"string","const":"response exceeded REST response size limit"},"max_response_bytes":{"type":"integer","minimum":1},"hint":{"type":"string"}},"additionalProperties":false}
    })
}

fn validate_openapi(root: &Path, value: &Value) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    if value.get("openapi").and_then(Value::as_str) != Some("3.1.0") {
        failures.push("OpenAPI version must be 3.1.0".to_owned());
    }
    for path in required_openapi_paths(root)? {
        if value
            .pointer(&format!("/paths/{}", escape_pointer(&path)))
            .is_none()
        {
            failures.push(format!("missing path {path}"));
        }
    }
    if let Some(paths) = value.get("paths").and_then(Value::as_object) {
        for (path, methods) in paths {
            if let Some(methods) = methods.as_object() {
                for (method, operation) in methods {
                    if operation.get("operationId").is_none_or(Value::is_null) {
                        failures.push(format!(
                            "{} {path} is missing operationId",
                            method.to_uppercase()
                        ));
                    }
                }
            }
        }
    }

    let entries = action_entries(root)?;
    if entries.len() != action_spec_count(root)? {
        failures.push(format!(
            "ActionSpec parser drifted: parsed {} entries from {} specs",
            entries.len(),
            action_spec_count(root)?
        ));
    }
    let expected: Vec<String> = entries
        .iter()
        .filter(|entry| entry.transport == "Any")
        .map(|entry| entry.name.clone())
        .collect();
    let action_enum = value
        .pointer("/components/schemas/ActionName/enum")
        .cloned()
        .unwrap_or(Value::Null);
    if action_enum != json!(expected) {
        failures.push(format!(
            "ActionName enum drifted: expected {expected:?}, got {action_enum}"
        ));
    }
    let missing_route_metadata: Vec<String> = entries
        .iter()
        .filter(|entry| entry.transport == "Any")
        .filter(|entry| entry.rest_method.is_none() || entry.rest_path.is_none())
        .map(|entry| entry.name.clone())
        .collect();
    if !missing_route_metadata.is_empty() {
        failures.push(format!(
            "ACTION_SPECS entries are missing REST route metadata: {missing_route_metadata:?}"
        ));
    }
    for name in entries
        .iter()
        .filter(|entry| entry.transport == "McpOnly")
        .map(|entry| &entry.name)
    {
        if action_enum
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(name)))
        {
            failures.push(format!(
                "MCP-only action {name:?} must not appear in REST ActionName enum"
            ));
        }
    }
    if value.pointer("/x-template/rest_actions") != Some(&json!(expected)) {
        failures.push(format!(
            "x-template rest_actions drifted: expected {expected:?}, got {}",
            value
                .pointer("/x-template/rest_actions")
                .unwrap_or(&Value::Null)
        ));
    }
    let expected_mcp_only: Vec<String> = entries
        .iter()
        .filter(|entry| entry.transport == "McpOnly")
        .map(|entry| entry.name.clone())
        .collect();
    if value.pointer("/x-template/mcp_only_actions") != Some(&json!(expected_mcp_only)) {
        failures.push("x-template mcp_only_actions drifted".to_owned());
    }
    for action in entries
        .iter()
        .filter(|entry| entry.transport == "Any")
        .filter(|entry| entry.rest_method.is_some() && entry.rest_path.is_some())
    {
        let method = action.rest_method.as_deref().unwrap().to_ascii_lowercase();
        let path = action.rest_path.as_deref().unwrap();
        if !expected.iter().any(|expected| expected == &action.name) {
            continue;
        }
        if value
            .pointer(&format!("/paths/{}/{}", escape_pointer(path), method))
            .is_none()
        {
            failures.push(format!(
                "missing direct REST operation {} {path}",
                method.to_uppercase()
            ));
        }
    }
    if value.pointer("/paths/~1v1~1example").is_some() {
        failures.push("/v1/example must not be present; REST uses direct routes only".to_owned());
    }
    if value.pointer("/paths/~1v1~1capabilities/get/responses/200/content/application~1json/schema")
        != Some(&schema_ref("CapabilitiesResponse"))
    {
        failures.push("/v1/capabilities must return CapabilitiesResponse".to_owned());
    }
    if value
        .pointer("/components/schemas/StatusResponse/properties/api_url")
        .is_some()
    {
        failures.push(
            "StatusResponse must not advertise api_url on the public status schema".to_owned(),
        );
    }
    Ok(failures)
}

fn render_schema_docs(root: &Path) -> Result<String> {
    let actions = action_entries(root)?;
    let mut lines = vec![
        "# MCP Schema Contract".to_owned(),
        "".to_owned(),
        "Generated from `crates/rtemplate-contracts/src/actions.rs` and checked against the schema, README, skill docs, help text, and scope routing.".to_owned(),
        "".to_owned(),
        "Run:".to_owned(),
        "".to_owned(),
        "```bash".to_owned(),
        "cargo xtask check-schema-docs --write".to_owned(),
        "cargo xtask check-schema-docs --check".to_owned(),
        "```".to_owned(),
        "".to_owned(),
        "## Tool".to_owned(),
        "".to_owned(),
        "| Field | Value |".to_owned(),
        "|---|---|".to_owned(),
        "| Tool name | `example` |".to_owned(),
        "| Schema resource | `example://schema/mcp-tool` |".to_owned(),
        "| Dispatch parameter | `action` |".to_owned(),
        "".to_owned(),
        "## Actions".to_owned(),
        "".to_owned(),
        "| Action | Scope | Cost | Description |".to_owned(),
        "|---|---|---|---|".to_owned(),
    ];
    for action in &actions {
        lines.push(format!(
            "| `{}` | {} | `{}` | {} |",
            action.name, action.doc_scope, action.cost, action.description
        ));
    }
    lines.extend(SCHEMA_DOC_TAIL.iter().map(|line| (*line).to_owned()));
    Ok(lines.join("\n"))
}

const SCHEMA_DOC_TAIL: &[&str] = &[
    "",
    "## Drift Rules",
    "",
    "- `ACTION_SPECS` in `crates/rtemplate-contracts/src/actions.rs` is the canonical action and scope list.",
    "- Action cost is planner metadata. Use `cheap` for first-pass reads, `moderate` for bounded workflow setup, `expensive` for broad scans or long-running work, and `write` for mutating operations.",
    "- `crates/rtemplate-mcp/src/schemas.rs` must derive its enum from `ACTION_SPECS`.",
    "- The MCP tool schema must reject unknown top-level parameters except reserved `_response_*` continuation fields, and encode action-specific requirements that fit the single-tool dispatch model.",
    "- `help` is intentionally public and must have no required scope.",
    "- `crates/rtemplate-mcp/src/tools.rs`, `README.md`, and `plugins/rtemplate/skills/example/SKILL.md` must mention every action.",
    "- `crates/rtemplate-mcp/src/rmcp_server.rs` owns stable resources and must keep `example://schema/mcp-tool` wired to `tool_definitions()`.",
    "- `crates/rtemplate-mcp/src/prompts.rs` owns stable prompts and must keep `quick_start` covered by prompt tests.",
    "",
    "## Resources",
    "",
    "| URI | Source | Contract |",
    "|---|---|---|",
    "| `example://schema/mcp-tool` | `crates/rtemplate-mcp/src/rmcp_server.rs` | Returns `tool_definitions()` as `application/json`. |",
    "",
    "## Prompts",
    "",
    "| Prompt | Source | Contract |",
    "|---|---|---|",
    "| `quick_start` | `crates/rtemplate-mcp/src/prompts.rs` | Guides a client to call `status` and `greet`. |",
    "",
    "## Input Validation",
    "",
    "- `action` is always required.",
    "- `echo` conditionally requires non-empty `message`.",
    "- `greet` accepts optional `name` and defaults to World.",
    "- `elicit_name` and `scaffold_intent` collect their extra fields through MCP elicitation, not direct tool-call arguments.",
    "- Unknown top-level parameters are rejected by the schema except reserved MCP adapter continuation fields.",
    "",
    "## Reserved Adapter Parameters",
    "",
    "Oversized MCP responses are returned as `kind=mcp_response_page` envelopes. Continuation calls reuse the same tool and original arguments, plus these reserved fields:",
    "",
    "| Parameter | Type | Purpose |",
    "|---|---|---|",
    "| `_response_cursor` | string | Cursor for cached serialized response data. Required with `_response_offset`. |",
    "| `_response_offset` | integer | Byte offset into the cached serialized response. |",
    "| `_response_page_bytes` | integer | Page size in bytes, from 1 to 16000. |",
    "",
    "The adapter strips these fields before dispatching to the service layer.",
    "",
];

fn check_schema_mentions(root: &Path, actions: &[ActionEntry]) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    for (label, path) in [
        ("README.md", root.join("README.md")),
        (
            "plugins/rtemplate/skills/rtemplate/SKILL.md",
            root.join("plugins/rtemplate/skills/rtemplate/SKILL.md"),
        ),
    ] {
        let text = read(&path)?;
        for action in actions {
            if !text.contains(&action.name) {
                failures.push(format!("{label} does not mention action `{}`", action.name));
            }
        }
    }
    let tools_text = read(root.join("crates/rtemplate-mcp/src/tools.rs"))?;
    if !tools_text.contains("ACTION_SPECS") || !tools_text.contains("build_help_text") {
        failures.push(
            "crates/rtemplate-mcp/src/tools.rs HELP_TEXT must be derived from ACTION_SPECS"
                .to_owned(),
        );
    }
    Ok(failures)
}

fn check_schema_scope(root: &Path, actions: &[ActionEntry]) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    let action_names: BTreeSet<&str> = actions.iter().map(|action| action.name.as_str()).collect();
    let scope_names: BTreeSet<&str> = actions.iter().map(|action| action.name.as_str()).collect();
    let cost_names: BTreeSet<&str> = actions.iter().map(|action| action.name.as_str()).collect();
    if scope_names != action_names {
        failures.push("ACTION_SPECS action names and scope entries are out of sync".to_owned());
    }
    if cost_names != action_names {
        failures.push("ACTION_SPECS action names and cost entries are out of sync".to_owned());
    }
    if actions
        .iter()
        .find(|action| action.name == "help")
        .map(|action| action.scope.as_str())
        != Some("public")
    {
        failures.push("help must be public".to_owned());
    }
    for action in actions.iter().filter(|action| action.name != "help") {
        if action.scope == "public" {
            failures.push(format!(
                "action `{}` must declare a required scope",
                action.name
            ));
        }
    }
    let schema_text = read(root.join("crates/rtemplate-mcp/src/schemas.rs"))?;
    if !schema_text.contains("tool_definitions_for_catalogs")
        || !schema_text.contains("action_names(catalogs)")
    {
        failures.push(
            "crates/rtemplate-mcp/src/schemas.rs must derive action enum from provider catalogs"
                .to_owned(),
        );
    }
    if !schema_text.contains("\"additionalProperties\": false") {
        failures.push(
            "crates/rtemplate-mcp/src/schemas.rs must reject unknown top-level properties"
                .to_owned(),
        );
    }
    if !schema_text.contains("required_param_conditionals(catalogs)")
        || !schema_text.contains("\"then\": { \"required\": required }")
    {
        failures.push("crates/rtemplate-mcp/src/schemas.rs must derive required action parameters from provider catalogs".to_owned());
    }
    let rmcp_server_text = read(root.join("crates/rtemplate-mcp/src/rmcp_server.rs"))?;
    if !rmcp_server_text.contains("example://schema/mcp-tool")
        || !rmcp_server_text.contains("tool_definitions_for_state")
    {
        failures.push("crates/rtemplate-mcp/src/rmcp_server.rs must expose the schema resource from the state-backed tool definitions".to_owned());
    }
    let prompts_text = read(root.join("crates/rtemplate-mcp/src/prompts.rs"))?;
    if !prompts_text.contains("quick_start") {
        failures
            .push("crates/rtemplate-mcp/src/prompts.rs must expose quick_start prompt".to_owned());
    }
    Ok(failures)
}

fn validate_scaffold_schema(path: &Path) -> Result<()> {
    let schema = load_json(path)?;
    require(
        schema.as_object().is_some(),
        format!("{}: schema root must be an object", path.display()),
    )?;
    require(
        schema.get("$schema").and_then(Value::as_str)
            == Some("https://json-schema.org/draft/2020-12/schema"),
        format!("{}: expected JSON Schema draft 2020-12", path.display()),
    )?;
    require(
        schema
            .pointer("/properties/kind/const")
            .and_then(Value::as_str)
            == Some("rmcp_template_scaffold_intent"),
        format!("{}: kind const drifted", path.display()),
    )?;
    let expected_required = str_set(&[
        "kind",
        "schema_version",
        "server_category",
        "required_surfaces",
        "project",
        "upstream",
        "runtime",
        "mcp_primitives",
        "deployment",
        "plugins",
        "publish_mcp",
        "crawl_docs",
        "handoff",
        "policy",
    ]);
    let required = string_set(schema.get("required").unwrap_or(&Value::Null));
    require(
        required == expected_required,
        format!(
            "{}: root required fields drifted: {:?}",
            path.display(),
            sorted_set(&required)
        ),
    )?;
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|map| map.keys().map(String::as_str).collect::<BTreeSet<_>>())
        .unwrap_or_default();
    require(
        expected_required.iter().all(|key| properties.contains(key)),
        format!(
            "{}: root properties missing required fields",
            path.display()
        ),
    )?;
    require(
        !properties.contains("actions"),
        format!("{}: legacy actions property must not exist", path.display()),
    )?;
    require(
        !schema.to_string().contains("resource_groups"),
        format!(
            "{}: legacy resource_groups field must not exist",
            path.display()
        ),
    )?;
    let auth_enum = string_set(
        schema
            .pointer("/properties/upstream/properties/auth_kind/enum")
            .unwrap_or(&Value::Null),
    );
    require(
        auth_enum == str_set(AUTH_KINDS),
        format!(
            "{}: auth_kind enum mismatch: {:?}",
            path.display(),
            sorted_set(&auth_enum)
        ),
    )?;
    Ok(())
}

fn validate_scaffold_payload(payload: &Value, source: &Path) -> Result<()> {
    let Some(obj) = payload.as_object() else {
        bail!("{}: root must be an object", source.display());
    };
    let root_keys = str_set(&[
        "kind",
        "schema_version",
        "server_category",
        "required_surfaces",
        "project",
        "upstream",
        "runtime",
        "mcp_primitives",
        "deployment",
        "plugins",
        "publish_mcp",
        "crawl_docs",
        "handoff",
        "policy",
    ]);
    require_keys(obj, source, "", &root_keys)?;
    require_no_extra(obj, source, "", &root_keys)?;
    require(
        payload.get("kind").and_then(Value::as_str) == Some("rmcp_template_scaffold_intent"),
        format!("{}: invalid kind", source.display()),
    )?;
    require(
        payload.get("schema_version").and_then(Value::as_i64) == Some(1),
        format!("{}: invalid schema_version", source.display()),
    )?;

    let category = required_str(payload, "server_category", source)?;
    require(
        matches!(category, "upstream-client" | "application-platform"),
        format!("{}: invalid server_category", source.display()),
    )?;
    let surfaces = unique_list(
        payload.get("required_surfaces").unwrap_or(&Value::Null),
        format!("{}: required_surfaces", source.display()),
        Some(SURFACES),
    )?;
    if category == "upstream-client" {
        require(
            surfaces == ["mcp", "cli"],
            format!(
                "{}: upstream-client must use ['mcp', 'cli']",
                source.display()
            ),
        )?;
    } else {
        require(
            string_slice_set(&surfaces) == str_set(&["api", "cli", "mcp", "web"]),
            format!(
                "{}: application-platform must include api, cli, mcp, web",
                source.display()
            ),
        )?;
    }

    validate_project(payload.get("project").unwrap_or(&Value::Null), source)?;
    validate_upstream(payload.get("upstream").unwrap_or(&Value::Null), source)?;
    validate_runtime(
        payload.get("runtime").unwrap_or(&Value::Null),
        source,
        category,
    )?;
    unique_list(
        payload.get("mcp_primitives").unwrap_or(&Value::Null),
        format!("{}: mcp_primitives", source.display()),
        Some(PRIMITIVES),
    )?;
    require(
        in_allowed(required_str(payload, "deployment", source)?, DEPLOYMENTS),
        format!("{}: invalid deployment", source.display()),
    )?;
    unique_list(
        payload.get("plugins").unwrap_or(&Value::Null),
        format!("{}: plugins", source.display()),
        Some(PLUGINS),
    )?;
    require(
        payload
            .get("publish_mcp")
            .and_then(Value::as_bool)
            .is_some(),
        format!("{}: publish_mcp must be boolean", source.display()),
    )?;
    validate_crawl(payload.get("crawl_docs").unwrap_or(&Value::Null), source)?;
    validate_handoff(payload.get("handoff").unwrap_or(&Value::Null), source)?;
    validate_policy(payload.get("policy").unwrap_or(&Value::Null), source)?;
    Ok(())
}

fn validate_project(value: &Value, source: &Path) -> Result<()> {
    let obj = object(value, source, "project")?;
    let keys = str_set(&[
        "display_name",
        "crate_name",
        "binary_name",
        "service_name",
        "env_prefix",
    ]);
    require_keys(obj, source, "project", &keys)?;
    require_no_extra(obj, source, "project", &keys)?;
    require(
        !required_obj_str(obj, "display_name", source, "project")?.is_empty(),
        format!("{}: project.display_name required", source.display()),
    )?;
    require(
        is_crate_name(required_obj_str(obj, "crate_name", source, "project")?),
        format!("{}: invalid crate_name", source.display()),
    )?;
    require(
        is_crate_name(required_obj_str(obj, "binary_name", source, "project")?),
        format!("{}: invalid binary_name", source.display()),
    )?;
    require(
        is_ident(required_obj_str(obj, "service_name", source, "project")?),
        format!("{}: invalid service_name", source.display()),
    )?;
    require(
        is_env(required_obj_str(obj, "env_prefix", source, "project")?),
        format!("{}: invalid env_prefix", source.display()),
    )?;
    Ok(())
}

fn validate_upstream(value: &Value, source: &Path) -> Result<()> {
    let obj = object(value, source, "upstream")?;
    let keys = str_set(&["base_url_env", "auth_kind"]);
    require_no_extra(obj, source, "upstream", &keys)?;
    require(
        is_api_url_env(required_obj_str(obj, "base_url_env", source, "upstream")?),
        format!("{}: invalid upstream.base_url_env", source.display()),
    )?;
    require(
        in_allowed(
            required_obj_str(obj, "auth_kind", source, "upstream")?,
            AUTH_KINDS,
        ),
        format!("{}: invalid auth_kind", source.display()),
    )?;
    Ok(())
}

fn validate_runtime(value: &Value, source: &Path, category: &str) -> Result<()> {
    let obj = object(value, source, "runtime")?;
    let keys = str_set(&["host", "port", "binary_profile", "mcp_transport"]);
    require_no_extra(obj, source, "runtime", &keys)?;
    require(
        !required_obj_str(obj, "host", source, "runtime")?.is_empty(),
        format!("{}: runtime.host required", source.display()),
    )?;
    let port = obj.get("port").and_then(Value::as_i64).unwrap_or_default();
    require(
        (1..=65535).contains(&port),
        format!("{}: runtime.port out of range", source.display()),
    )?;
    let profile = required_obj_str(obj, "binary_profile", source, "runtime")?;
    require(
        in_allowed(profile, BINARY_PROFILES),
        format!("{}: invalid runtime.binary_profile", source.display()),
    )?;
    if category == "upstream-client" {
        require(
            profile == "local-adapter",
            format!(
                "{}: upstream-client must default to local-adapter binary profile",
                source.display()
            ),
        )?;
    } else {
        require(
            profile == "server-full",
            format!(
                "{}: application-platform must default to server-full binary profile",
                source.display()
            ),
        )?;
    }
    require(
        in_allowed(
            required_obj_str(obj, "mcp_transport", source, "runtime")?,
            TRANSPORTS,
        ),
        format!("{}: invalid runtime.mcp_transport", source.display()),
    )?;
    Ok(())
}

fn validate_crawl(value: &Value, source: &Path) -> Result<()> {
    let obj = object(value, source, "crawl_docs")?;
    let keys = str_set(&["urls", "repos", "search_topics"]);
    require_no_extra(obj, source, "crawl_docs", &keys)?;
    for key in ["urls", "repos", "search_topics"] {
        let values = unique_list(
            obj.get(key).unwrap_or(&Value::Null),
            format!("{}: crawl_docs.{key}", source.display()),
            None,
        )?;
        require(
            values.iter().all(|item| !item.is_empty()),
            format!(
                "{}: crawl_docs.{key} entries must be non-empty strings",
                source.display()
            ),
        )?;
        if matches!(key, "urls" | "repos") {
            require(
                values.iter().all(|item| is_uri(item)),
                format!(
                    "{}: crawl_docs.{key} entries must be URIs",
                    source.display()
                ),
            )?;
        }
    }
    Ok(())
}

fn validate_handoff(value: &Value, source: &Path) -> Result<()> {
    let obj = object(value, source, "handoff")?;
    let keys = str_set(&["recommended_skill", "instructions"]);
    require_keys(obj, source, "handoff", &keys)?;
    require_no_extra(obj, source, "handoff", &keys)?;
    require(
        required_obj_str(obj, "recommended_skill", source, "handoff")? == "scaffold-project",
        format!(
            "{}: handoff.recommended_skill must be scaffold-project",
            source.display()
        ),
    )?;
    require(
        required_obj_str(obj, "instructions", source, "handoff")?
            .to_lowercase()
            .contains("approve"),
        format!(
            "{}: handoff instructions must mention approval",
            source.display()
        ),
    )?;
    Ok(())
}

fn validate_policy(value: &Value, source: &Path) -> Result<()> {
    let obj = object(value, source, "policy")?;
    let keys = str_set(&[
        "business_action_minimum_surfaces",
        "upstream_client_surfaces",
        "application_platform_surfaces",
        "binary_profiles",
    ]);
    require_keys(obj, source, "policy", &keys)?;
    require_no_extra(obj, source, "policy", &keys)?;
    require(
        string_vec(
            obj.get("business_action_minimum_surfaces")
                .unwrap_or(&Value::Null),
        ) == ["mcp", "cli"],
        format!(
            "{}: business action minimum must be ['mcp', 'cli']",
            source.display()
        ),
    )?;
    require(
        string_vec(obj.get("upstream_client_surfaces").unwrap_or(&Value::Null)) == ["mcp", "cli"],
        format!("{}: upstream policy mismatch", source.display()),
    )?;
    require(
        string_slice_set(&string_vec(
            obj.get("application_platform_surfaces")
                .unwrap_or(&Value::Null),
        )) == str_set(&["api", "cli", "mcp", "web"]),
        format!("{}: application policy mismatch", source.display()),
    )?;
    let profiles = object(
        obj.get("binary_profiles").unwrap_or(&Value::Null),
        source,
        "policy.binary_profiles",
    )?;
    let profile_keys = str_set(&[
        "upstream_client_default",
        "application_platform_default",
        "gateway_shared_default",
    ]);
    require_no_extra(profiles, source, "policy.binary_profiles", &profile_keys)?;
    require(
        required_obj_str(
            profiles,
            "upstream_client_default",
            source,
            "policy.binary_profiles",
        )? == "local-adapter",
        format!(
            "{}: upstream binary profile policy mismatch",
            source.display()
        ),
    )?;
    require(
        required_obj_str(
            profiles,
            "application_platform_default",
            source,
            "policy.binary_profiles",
        )? == "server-full",
        format!(
            "{}: application binary profile policy mismatch",
            source.display()
        ),
    )?;
    require(
        required_obj_str(
            profiles,
            "gateway_shared_default",
            source,
            "policy.binary_profiles",
        )? == "server-full",
        format!(
            "{}: gateway binary profile policy mismatch",
            source.display()
        ),
    )?;
    Ok(())
}

fn action_entries(root: &Path) -> Result<Vec<ActionEntry>> {
    let text = read(root.join("crates/rtemplate-contracts/src/actions.rs"))?;
    Ok(parse_action_entries(&text))
}

fn extract_actions(root: &Path) -> Result<Vec<ActionEntry>> {
    action_entries(root)
}

fn parse_action_entries(text: &str) -> Vec<ActionEntry> {
    action_blocks(text)
        .into_iter()
        .filter_map(|entry| {
            let name = field_string(entry, "name")?;
            let description = field_string(entry, "description")?;
            let scope_expr = field_expr(entry, "required_scope")?;
            let transport = enum_variant(entry, "transport", "ActionTransport")?;
            let rest_method = option_string_expr(field_expr(entry, "rest_method")?.as_str())?;
            let rest_path = option_string_expr(field_expr(entry, "rest_path")?.as_str())?;
            let cost = enum_variant(entry, "cost", "ActionCost")?.to_lowercase();
            let (scope, doc_scope) = match scope_expr.trim() {
                "None" => ("public".to_owned(), "public".to_owned()),
                "Some(READ_SCOPE)" => ("example:read".to_owned(), "`example:read`".to_owned()),
                "Some(WRITE_SCOPE)" => ("example:write".to_owned(), "`example:write`".to_owned()),
                _ => (
                    "example:__deny__".to_owned(),
                    "`example:__deny__`".to_owned(),
                ),
            };
            Some(ActionEntry {
                name,
                description,
                scope,
                doc_scope,
                transport,
                rest_method,
                rest_path,
                cost,
            })
        })
        .collect()
}

fn action_spec_count(root: &Path) -> Result<usize> {
    let text = read(root.join("crates/rtemplate-contracts/src/actions.rs"))?;
    Ok(action_blocks(&text)
        .into_iter()
        .filter(|block| block.trim_start().starts_with("name:"))
        .count())
}

fn action_blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("ActionSpec") {
        rest = &rest[start + "ActionSpec".len()..];
        let Some(open) = rest.find('{') else {
            break;
        };
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else {
            break;
        };
        blocks.push(&rest[..close]);
        rest = &rest[close + 1..];
    }
    blocks
}

fn field_string(entry: &str, name: &str) -> Option<String> {
    let marker = format!("{name}:");
    let start = entry.find(&marker)? + marker.len();
    let after = entry[start..].trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_owned())
}

fn field_expr(entry: &str, name: &str) -> Option<String> {
    let marker = format!("{name}:");
    let start = entry.find(&marker)? + marker.len();
    let after = entry[start..].trim_start();
    let end = after.find([',', '\n']).unwrap_or(after.len());
    Some(after[..end].trim().to_owned())
}

fn enum_variant(entry: &str, field: &str, enum_name: &str) -> Option<String> {
    let expr = field_expr(entry, field)?;
    let marker = format!("{enum_name}::");
    let variant = expr.strip_prefix(&marker)?;
    Some(variant.trim().to_owned())
}

fn package_version(root: &Path) -> Result<String> {
    for manifest in [
        root.join("crates/rmcp-template/Cargo.toml"),
        root.join("Cargo.toml"),
    ] {
        if !manifest.exists() {
            continue;
        }
        let text = read(&manifest)?;
        for line in text.lines() {
            let line = line.trim();
            if let Some(value) = line
                .strip_prefix("version")
                .and_then(|line| line.trim_start().strip_prefix('='))
            {
                return Ok(value.trim().trim_matches('"').to_owned());
            }
        }
    }
    bail!("could not find package version in Cargo.toml")
}

fn default_mcp_port(root: &Path) -> Result<u16> {
    let text = read(root.join("crates/rtemplate-contracts/src/config.rs"))?;
    let Some(start) = text.find("fn default_mcp_port()") else {
        bail!("could not find default_mcp_port in config.rs");
    };
    let after = &text[start..];
    let Some(open) = after.find('{') else {
        bail!("could not parse default_mcp_port in config.rs");
    };
    let Some(close) = after[open + 1..].find('}') else {
        bail!("could not parse default_mcp_port in config.rs");
    };
    let value = after[open + 1..open + 1 + close].trim();
    value
        .parse::<u16>()
        .with_context(|| format!("could not parse default_mcp_port value {value:?}"))
}

fn option_string_expr(value: &str) -> Option<Option<String>> {
    let value = value.trim();
    if value == "None" {
        return Some(None);
    }
    let inner = value.strip_prefix("Some(\"")?.strip_suffix("\")")?;
    Some(Some(inner.to_owned()))
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn param_example(action: &str) -> Value {
    match action {
        "greet" => json!({"name":"Alice"}),
        "echo" => json!({"message":"Hello!"}),
        _ => json!({}),
    }
}

fn rest_schemas(action: &str) -> Option<(Option<&'static str>, &'static str)> {
    REST_SCHEMAS
        .iter()
        .find(|(name, _, _)| *name == action)
        .map(|(_, request, response)| (*request, *response))
}

fn title_no_underscore(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect()
}

fn required_openapi_paths(root: &Path) -> Result<Vec<String>> {
    let mut paths = vec![
        "/health".to_owned(),
        "/openapi.json".to_owned(),
        "/status".to_owned(),
        "/v1/capabilities".to_owned(),
    ];
    paths.extend(
        action_entries(root)?
            .into_iter()
            .filter(|entry| entry.transport == "Any")
            .filter_map(|entry| entry.rest_path),
    );
    Ok(paths)
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn load_json(path: &Path) -> Result<Value> {
    let text = read(path)?;
    serde_json::from_str(&text).with_context(|| format!("{}: invalid JSON", path.display()))
}

fn read(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn canonical_json(value: &Value) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

fn current_dir() -> Result<PathBuf> {
    std::env::current_dir().context("failed to read current directory")
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn finish_failures(failures: Vec<String>) -> Result<()> {
    if failures.is_empty() {
        return Ok(());
    }
    for failure in &failures {
        eprintln!("FAIL: {failure}");
    }
    bail!("check failed")
}

fn require(condition: bool, message: String) -> Result<()> {
    if condition {
        Ok(())
    } else {
        bail!(message)
    }
}

fn object<'a>(
    value: &'a Value,
    source: &Path,
    path: &str,
) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .as_object()
        .with_context(|| format!("{}: {path} must be object", source.display()))
}

fn require_keys(
    obj: &serde_json::Map<String, Value>,
    source: &Path,
    path: &str,
    keys: &BTreeSet<&str>,
) -> Result<()> {
    let actual: BTreeSet<&str> = obj.keys().map(String::as_str).collect();
    let missing: Vec<&str> = keys.difference(&actual).copied().collect();
    let label = if path.is_empty() {
        source.display().to_string()
    } else {
        format!("{}: {path}", source.display())
    };
    require(
        missing.is_empty(),
        format!("{label}: missing required keys: {missing:?}"),
    )
}

fn require_no_extra(
    obj: &serde_json::Map<String, Value>,
    source: &Path,
    path: &str,
    keys: &BTreeSet<&str>,
) -> Result<()> {
    let actual: BTreeSet<&str> = obj.keys().map(String::as_str).collect();
    let extra: Vec<&str> = actual.difference(keys).copied().collect();
    let label = if path.is_empty() {
        source.display().to_string()
    } else {
        format!("{}: {path}", source.display())
    };
    require(
        extra.is_empty(),
        format!("{label}: unexpected keys: {extra:?}"),
    )
}

fn required_str<'a>(payload: &'a Value, key: &str, source: &Path) -> Result<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{}: {key} required", source.display()))
}

fn required_obj_str<'a>(
    obj: &'a serde_json::Map<String, Value>,
    key: &str,
    source: &Path,
    path: &str,
) -> Result<&'a str> {
    obj.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{}: {path}.{key} required", source.display()))
}

fn unique_list(value: &Value, path: String, allowed: Option<&[&str]>) -> Result<Vec<String>> {
    let Some(items) = value.as_array() else {
        bail!("{path}: expected list");
    };
    let values: Vec<String> = items
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    require(
        values.len() == items.len(),
        format!("{path}: entries must be strings"),
    )?;
    let set: BTreeSet<&str> = values.iter().map(String::as_str).collect();
    require(
        values.len() == set.len(),
        format!("{path}: duplicate values are not allowed"),
    )?;
    if let Some(allowed) = allowed {
        let invalid: Vec<&str> = set
            .iter()
            .copied()
            .filter(|item| !in_allowed(item, allowed))
            .collect();
        require(
            invalid.is_empty(),
            format!(
                "{path}: invalid values: {invalid:?}; allowed={:?}",
                sorted_slice(allowed)
            ),
        )?;
    }
    Ok(values)
}

fn string_vec(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn string_set(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn str_set<'a>(items: &'a [&'a str]) -> BTreeSet<&'a str> {
    items.iter().copied().collect()
}

fn string_slice_set(items: &[String]) -> BTreeSet<&str> {
    items.iter().map(String::as_str).collect()
}

fn sorted_set(set: &BTreeSet<&str>) -> Vec<String> {
    set.iter().map(|item| (*item).to_owned()).collect()
}

fn sorted_slice<'a>(items: &'a [&'a str]) -> Vec<&'a str> {
    let mut items = items.to_vec();
    items.sort_unstable();
    items
}

fn in_allowed(value: &str, allowed: &[&str]) -> bool {
    allowed.contains(&value)
}

fn is_uri(value: &str) -> bool {
    for scheme in ["http://", "https://", "ssh://", "git://"] {
        if let Some(rest) = value.strip_prefix(scheme) {
            return !rest.split('/').next().unwrap_or("").is_empty();
        }
    }
    false
}

fn is_crate_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn is_env(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

fn is_api_url_env(value: &str) -> bool {
    is_env(value) && value.ends_with("_API_URL")
}

#[cfg(test)]
mod tests {
    use super::{
        allowed_unicode, escape_char, is_api_url_env, is_crate_name, is_ident, is_uri,
        parse_action_entries, substitution, title_no_underscore,
    };

    #[test]
    fn parses_action_specs_like_python_regex() {
        let text = r#"
            ActionSpec {
                name: "greet",
                description: "Return a greeting.",
                required_scope: Some(READ_SCOPE),
                transport: ActionTransport::Any,
                rest_method: Some("POST"),
                rest_path: Some("/v1/greet"),
                cost: ActionCost::Cheap,
            },
            ActionSpec {
                name: "elicit_name",
                description: "Ask for a name.",
                required_scope: Some(WRITE_SCOPE),
                transport: ActionTransport::McpOnly,
                rest_method: None,
                rest_path: None,
                cost: ActionCost::Moderate,
            },
            ActionSpec {
                name: "help",
                description: "Show help.",
                required_scope: None,
                transport: ActionTransport::Any,
                rest_method: Some("GET"),
                rest_path: Some("/v1/help"),
                cost: ActionCost::Cheap,
            },
        "#;
        let entries = parse_action_entries(text);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "greet");
        assert_eq!(entries[0].description, "Return a greeting.");
        assert_eq!(entries[0].scope, "example:read");
        assert_eq!(entries[0].rest_method.as_deref(), Some("POST"));
        assert_eq!(entries[0].rest_path.as_deref(), Some("/v1/greet"));
        assert_eq!(entries[1].transport, "McpOnly");
        assert_eq!(entries[1].rest_method, None);
        assert_eq!(entries[2].doc_scope, "public");
    }

    #[test]
    fn ascii_allowlist_and_substitutions_match_script_policy() {
        assert!(allowed_unicode(0x2713));
        assert!(!allowed_unicode(0x00E9));
        assert_eq!(substitution(0x2014), Some("-"));
        assert_eq!(substitution(0x2026), Some("..."));
        assert_eq!(escape_char('\\'), "\\\\");
    }

    #[test]
    fn scaffold_regex_replacements_match_python_patterns() {
        assert!(is_crate_name("foo-bar1"));
        assert!(!is_crate_name("Foo"));
        assert!(is_ident("foo_bar1"));
        assert!(!is_ident("foo-bar"));
        assert!(is_api_url_env("FOO_API_URL"));
        assert!(!is_api_url_env("FOO_URL"));
        assert!(is_uri("https://example.com/docs"));
        assert!(!is_uri("mailto:test@example.com"));
    }

    #[test]
    fn openapi_operation_id_title_matches_python_title_replace() {
        assert_eq!(title_no_underscore("scaffold_intent"), "ScaffoldIntent");
        assert_eq!(title_no_underscore("greet"), "Greet");
    }
}
