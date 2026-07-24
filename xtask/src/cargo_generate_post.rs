use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const TEXT_SUFFIXES: &[&str] = &[
    "css", "html", "json", "lock", "md", "mjs", "py", "rs", "rhai", "sh", "toml", "ts", "tsx",
    "txt", "yml", "yaml",
];

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".beads",
    ".cache",
    ".dolt",
    ".full-review",
    ".lavra",
    ".next",
    ".serena",
    ".superpowers",
    ".worktrees",
    "node_modules",
    "target",
    "dist",
];

const SKIP_FILES: &[&str] = &["cargo-generate.toml", "Cargo.lock"];

#[derive(Debug)]
struct Values {
    crate_name: String,
    crate_name_snake: String,
    crate_prefix: String,
    crate_prefix_snake: String,
    binary_name: String,
    service_slug: String,
    type_prefix: String,
    env_prefix: String,
    scope_prefix: String,
    default_port: String,
    default_feature_array: String,
    github_slug: String,
    github_url: String,
    github_ssh: String,
    mcp_surface_crate: String,
    mcp_surface_crate_snake: String,
}

#[derive(Debug, Deserialize)]
struct GeneratedValues {
    package_name: String,
    crate_prefix: String,
    binary_name: String,
    service_slug: String,
    type_prefix: String,
    env_prefix: String,
    scope_prefix: String,
    default_port: String,
    github_owner: String,
    github_repo: String,
    default_features: String,
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    if args.len() != 1 {
        bail!("Usage: cargo xtask cargo-generate-post <generated-root>");
    }

    let root = PathBuf::from(&args[0]);
    if !root.is_dir() {
        bail!("generated-root is not a directory: {}", root.display());
    }
    ensure_unprocessed_template_root(&root)?;
    let values = Values::from_generated_file(&root)?;
    let pairs = replacements(&values);
    rewrite_tree(&root, &pairs)?;
    rename_paths(&root, &values)?;
    cleanup_template_files(&root)?;
    cleanup_generated_readme(&root)?;
    ensure_agent_memory_symlinks(&root)?;
    Ok(())
}

impl Values {
    fn from_generated_file(root: &Path) -> Result<Self> {
        let path = root.join(".cargo-generate-values.toml");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let values = toml::from_str::<GeneratedValues>(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Self::parse(values)
    }

    fn parse(input: GeneratedValues) -> Result<Self> {
        validate_slug("package_name", &input.package_name)?;
        validate_slug("crate_prefix", &input.crate_prefix)?;
        validate_slug("binary_name", &input.binary_name)?;
        validate_identifier("service_slug", &input.service_slug)?;
        validate_type_prefix(&input.type_prefix)?;
        validate_env_prefix(&input.env_prefix)?;
        validate_scope_prefix(&input.scope_prefix)?;
        validate_port(&input.default_port)?;
        validate_github_name("github_owner", &input.github_owner)?;
        validate_github_name("github_repo", &input.github_repo)?;
        let features = validate_default_features(&input.default_features)?;
        let mcp_surface_crate = format!("{}-mcp-surface", input.crate_prefix);

        Ok(Self {
            crate_name_snake: snake(&input.package_name),
            crate_prefix_snake: snake(&input.crate_prefix),
            default_feature_array: cargo_feature_array(&features),
            github_slug: format!("{}/{}", input.github_owner, input.github_repo),
            github_url: format!(
                "https://github.com/{}/{}",
                input.github_owner, input.github_repo
            ),
            github_ssh: format!(
                "github.com:{}/{}.git",
                input.github_owner, input.github_repo
            ),
            mcp_surface_crate_snake: snake(&mcp_surface_crate),
            crate_name: input.package_name,
            crate_prefix: input.crate_prefix,
            binary_name: input.binary_name,
            service_slug: input.service_slug,
            type_prefix: input.type_prefix,
            env_prefix: input.env_prefix,
            scope_prefix: input.scope_prefix,
            default_port: input.default_port,
            mcp_surface_crate,
        })
    }
}

fn ensure_unprocessed_template_root(root: &Path) -> Result<()> {
    for relative in [
        "Cargo.toml",
        ".cargo-generate-values.toml",
        "apps/soma/Cargo.toml",
    ] {
        let path = root.join(relative);
        if !path.exists() {
            bail!(
                "generated-root does not look like an unprocessed soma output; missing {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn snake(value: &str) -> String {
    value.replace('-', "_")
}

fn validate_slug(name: &str, value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{name} must not be empty");
    };
    if !first.is_ascii_lowercase()
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        bail!("{name} must match ^[a-z][a-z0-9_-]*$: {value:?}");
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

fn validate_type_prefix(value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("type_prefix must not be empty");
    };
    if !first.is_ascii_uppercase() || !chars.all(|ch| ch.is_ascii_alphanumeric()) {
        bail!("type_prefix must match ^[A-Z][A-Za-z0-9]*$: {value:?}");
    }
    Ok(())
}

fn validate_env_prefix(value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("env_prefix must not be empty");
    };
    if !first.is_ascii_uppercase()
        || !chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        bail!("env_prefix must match ^[A-Z][A-Z0-9_]*$: {value:?}");
    }
    Ok(())
}

fn validate_scope_prefix(value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("scope_prefix must not be empty");
    };
    if !first.is_ascii_lowercase()
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        bail!("scope_prefix must match ^[a-z][a-z0-9-]*$: {value:?}");
    }
    Ok(())
}

fn validate_port(value: &str) -> Result<()> {
    let port: u16 = value
        .parse()
        .with_context(|| format!("default_port must be an integer: {value:?}"))?;
    if port == 0 {
        bail!("default_port must be between 1 and 65535: {value:?}");
    }
    Ok(())
}

fn validate_github_name(name: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-')
    {
        bail!("{name} must match ^[A-Za-z0-9_.-]+$: {value:?}");
    }
    Ok(())
}

fn validate_default_features(value: &str) -> Result<Vec<String>> {
    const ALLOWED: &[&str] = &[
        "cli",
        "mcp",
        "mcp-stdio",
        "api",
        "auth",
        "oauth",
        "observability",
        "plugin",
        "mcp-http",
        "web",
        "local-adapter",
        "server",
        "full",
        "test-support",
    ];

    let features = value
        .split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if features.is_empty() {
        bail!("default_features must include at least one feature");
    }
    let unknown = features
        .iter()
        .filter(|feature| !ALLOWED.contains(&feature.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        bail!(
            "default_features contains unknown feature(s): {}",
            unknown.join(", ")
        );
    }
    Ok(features)
}

fn cargo_feature_array(features: &[String]) -> String {
    features
        .iter()
        .map(|feature| format!("\"{feature}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn replacements(values: &Values) -> Vec<(String, String)> {
    vec![
        (
            "https://github.com/your-org/soma-mcp".into(),
            values.github_url.clone(),
        ),
        (
            "https://github.com/dinglebear-ai/soma".into(),
            values.github_url.clone(),
        ),
        (
            "https://github.com/jmagar/soma".into(),
            values.github_url.clone(),
        ),
        (
            "https://github.com/jmagar/soma-mcp".into(),
            values.github_url.clone(),
        ),
        (
            "github.com:dinglebear-ai/soma.git".into(),
            values.github_ssh.clone(),
        ),
        (
            "github.com:jmagar/soma-mcp.git".into(),
            values.github_ssh.clone(),
        ),
        ("dinglebear-ai/soma".into(), values.github_slug.clone()),
        ("jmagar/soma".into(), values.github_slug.clone()),
        ("jmagar/soma-mcp".into(), values.github_slug.clone()),
        (
            "\"name\": \"soma-mcp\"".into(),
            format!("\"name\": \"{}\"", values.crate_name),
        ),
        (
            "[package]\nname = \"soma\"".into(),
            format!("[package]\nname = \"{}\"", values.crate_name),
        ),
        (
            "name = \"soma\"\npath = \"src/bin/soma.rs\"".into(),
            format!(
                "name = \"{}\"\npath = \"src/bin/{}.rs\"",
                values.binary_name, values.binary_name
            ),
        ),
        (
            "soma = { path = \".\", package = \"soma\", features = [\"test-support\"] }".into(),
            format!(
                "{} = {{ path = \".\", package = \"{}\", features = [\"test-support\"] }}",
                values.crate_name_snake, values.crate_name
            ),
        ),
        (
            "CARGO_BIN_EXE_soma".into(),
            format!("CARGO_BIN_EXE_{}", values.binary_name),
        ),
        (
            "default = [\"full\"]".into(),
            format!("default = [{}]", values.default_feature_array),
        ),
        ("soma".into(), values.crate_name_snake.clone()),
        ("soma_mcp".into(), values.mcp_surface_crate_snake.clone()),
        ("soma-mcp".into(), values.mcp_surface_crate.clone()),
        ("soma_".into(), format!("{}_", values.crate_prefix_snake)),
        ("soma-".into(), format!("{}-", values.crate_prefix)),
        ("soma".into(), values.binary_name.clone()),
        ("soma".into(), values.crate_name.clone()),
        ("SOMA".into(), values.env_prefix.clone()),
        (
            "SomaRmcpServer".into(),
            format!("{}RmcpServer", values.type_prefix),
        ),
        (
            "SomaService".into(),
            format!("{}Service", values.type_prefix),
        ),
        ("SomaClient".into(), format!("{}Client", values.type_prefix)),
        ("SomaConfig".into(), format!("{}Config", values.type_prefix)),
        ("SomaAction".into(), format!("{}Action", values.type_prefix)),
        (
            "apps/soma/src/bin/soma.rs".into(),
            format!(
                "apps/{}/src/bin/{}.rs",
                values.crate_name, values.binary_name
            ),
        ),
        ("soma:read".into(), format!("{}:read", values.scope_prefix)),
        (
            "soma:write".into(),
            format!("{}:write", values.scope_prefix),
        ),
        (
            "soma:__deny__".into(),
            format!("{}:__deny__", values.scope_prefix),
        ),
        ("soma".into(), values.service_slug.clone()),
        ("40060".into(), values.default_port.clone()),
        ("40000".into(), values.default_port.clone()),
        ("MyService".into(), values.type_prefix.clone()),
        ("myservice-mcp".into(), values.crate_name.clone()),
        ("myservice".into(), values.service_slug.clone()),
        ("MYSERVICE".into(), values.env_prefix.clone()),
    ]
}

fn rewrite_tree(root: &Path, pairs: &[(String, String)]) -> Result<()> {
    for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        !should_skip_dir(relative)
    }) {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type().is_file() || !should_rewrite(path) {
            continue;
        }
        let Ok(original) = fs::read_to_string(path) else {
            continue;
        };
        let mut updated = original.clone();
        for (old, new) in pairs {
            updated = updated.replace(old, new);
        }
        if updated != original {
            fs::write(path, updated)
                .with_context(|| format!("failed to rewrite {}", path.display()))?;
        }
    }
    Ok(())
}

fn rename_paths(root: &Path, values: &Values) -> Result<()> {
    let mut renames = Vec::new();
    for entry in WalkDir::new(root)
        .contents_first(true)
        .into_iter()
        .filter_entry(|entry| {
            let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
            !should_skip_dir(relative)
        })
    {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let new_name = rename_path_segment(path, values);
        if new_name != name {
            renames.push((path.to_path_buf(), path.with_file_name(new_name)));
        }
    }

    for (src, dst) in renames {
        if src.exists() && !dst.exists() {
            fs::rename(&src, &dst).with_context(|| {
                format!("failed to rename {} to {}", src.display(), dst.display())
            })?;
        }
    }
    Ok(())
}

fn rename_path_segment(path: &Path, values: &Values) -> String {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.display().to_string();
    };
    match name {
        "soma" if path_parent_name(path).is_some_and(|parent| parent == "plugins") => {
            values.binary_name.clone()
        }
        "soma" if path_parent_name(path).is_some_and(|parent| parent == "skills") => {
            values.binary_name.clone()
        }
        "soma" if path_parent_name(path).is_some_and(|parent| parent == "crates") => {
            values.crate_name_snake.clone()
        }
        "soma" => values.crate_name.clone(),
        "soma.rs" if path_parent_name(path).is_some_and(|parent| parent == "bin") => {
            format!("{}.rs", values.binary_name)
        }
        "soma.rs" => format!("{}.rs", values.crate_name_snake),
        "soma-rmcp" => values.crate_name.clone(),
        "soma-rmcp.js" => format!("{}.js", values.crate_name),
        "soma-mcp" => format!("{}-mcp", values.crate_name_snake),
        "soma_mcp" => format!("{}_mcp", values.crate_name_snake),
        _ if name.starts_with("soma-") => {
            format!("{}-{}", values.crate_name_snake, &name["soma-".len()..])
        }
        _ if name.starts_with("soma_") => {
            format!("{}_{}", values.crate_name_snake, &name["soma_".len()..])
        }
        _ => name.to_owned(),
    }
}

fn path_parent_name(path: &Path) -> Option<&str> {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
}

fn cleanup_template_files(root: &Path) -> Result<()> {
    for relative in [
        ".cargo-generate-values.toml",
        "cargo-generate.toml",
        "scaffold",
        "docs/CARGO_GENERATE.md",
    ] {
        let path = root.join(relative);
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        } else if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    let target = root.join("target");
    if target.exists() {
        let _ = fs::remove_dir_all(target);
    }
    Ok(())
}

fn cleanup_generated_readme(root: &Path) -> Result<()> {
    let path = root.join("README.md");
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;
    let Some(start) = text.find("\n## Generate a New Server\n") else {
        return Ok(());
    };
    let next = text[start + 1..]
        .find("\n## ")
        .map(|offset| start + 1 + offset)
        .unwrap_or(text.len());
    let mut updated = String::with_capacity(text.len());
    updated.push_str(&text[..start]);
    updated.push('\n');
    updated.push_str(&text[next..]);
    fs::write(path, updated)?;
    Ok(())
}

fn ensure_agent_memory_symlinks(root: &Path) -> Result<()> {
    for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        !should_skip_dir(relative)
    }) {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "CLAUDE.md" {
            continue;
        }
        let dir = entry
            .path()
            .parent()
            .with_context(|| format!("{} has no parent", entry.path().display()))?;
        for link_name in ["AGENTS.md", "GEMINI.md"] {
            let link = dir.join(link_name);
            if link.exists() || link.symlink_metadata().is_ok() {
                continue;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink("CLAUDE.md", &link)
                .with_context(|| format!("failed to create {}", link.display()))?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file("CLAUDE.md", &link)
                .with_context(|| format!("failed to create {}", link.display()))?;
        }
    }
    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        SKIP_DIRS.contains(&name.as_ref())
    })
}

fn should_rewrite(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| SKIP_FILES.contains(&name))
    {
        return false;
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| TEXT_SUFFIXES.contains(&extension))
    {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                "Dockerfile"
                    | "Justfile"
                    | "LICENSE"
                    | "README"
                    | "CLAUDE.md"
                    | "AGENTS.md"
                    | "GEMINI.md"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_values() -> Values {
        Values {
            crate_name: "myservice-mcp".to_owned(),
            crate_name_snake: "myservice_mcp".to_owned(),
            crate_prefix: "myservice".to_owned(),
            crate_prefix_snake: "myservice".to_owned(),
            binary_name: "myservice".to_owned(),
            service_slug: "myservice".to_owned(),
            type_prefix: "MyService".to_owned(),
            env_prefix: "MYSERVICE".to_owned(),
            scope_prefix: "myservice".to_owned(),
            default_port: "41234".to_owned(),
            default_feature_array: "\"full\"".to_owned(),
            github_slug: "jmagar/myservice-mcp".to_owned(),
            github_url: "https://github.com/jmagar/myservice-mcp".to_owned(),
            github_ssh: "github.com:jmagar/myservice-mcp.git".to_owned(),
            mcp_surface_crate: "myservice-mcp-surface".to_owned(),
            mcp_surface_crate_snake: "myservice_mcp_surface".to_owned(),
        }
    }

    #[test]
    fn rename_paths_maps_soma_root_crate_and_support_packages() {
        let fixture = TempDir::new().unwrap();
        fs::create_dir_all(fixture.path().join("apps/soma/src/bin")).unwrap();
        fs::create_dir_all(fixture.path().join("crates/soma/api")).unwrap();
        fs::create_dir_all(fixture.path().join("packages/soma-rmcp/bin")).unwrap();
        fs::create_dir_all(fixture.path().join("plugins/soma/skills/soma")).unwrap();
        fs::write(fixture.path().join("apps/soma/Cargo.toml"), "").unwrap();
        fs::write(fixture.path().join("apps/soma/src/bin/soma.rs"), "").unwrap();
        fs::write(fixture.path().join("crates/soma/api/Cargo.toml"), "").unwrap();
        fs::write(fixture.path().join("packages/soma-rmcp/package.json"), "").unwrap();
        fs::write(
            fixture.path().join("packages/soma-rmcp/bin/soma-rmcp.js"),
            "",
        )
        .unwrap();
        fs::write(fixture.path().join("plugins/soma/.claude-plugin.json"), "").unwrap();
        fs::write(fixture.path().join("plugins/soma/skills/soma/SKILL.md"), "").unwrap();

        rename_paths(fixture.path(), &test_values()).unwrap();

        assert!(fixture
            .path()
            .join("apps/myservice-mcp/Cargo.toml")
            .exists());
        assert!(fixture
            .path()
            .join("apps/myservice-mcp/src/bin/myservice.rs")
            .exists());
        assert!(fixture
            .path()
            .join("crates/myservice_mcp/api/Cargo.toml")
            .exists());
        assert!(fixture
            .path()
            .join("packages/myservice-mcp/package.json")
            .exists());
        assert!(fixture
            .path()
            .join("packages/myservice-mcp/bin/myservice-mcp.js")
            .exists());
        assert!(fixture
            .path()
            .join("plugins/myservice/.claude-plugin.json")
            .exists());
        assert!(fixture
            .path()
            .join("plugins/myservice/skills/myservice/SKILL.md")
            .exists());
    }

    #[test]
    fn agent_memory_symlinks_are_recreated_after_cargo_generate_skips_them() {
        let fixture = TempDir::new().unwrap();
        fs::write(fixture.path().join("CLAUDE.md"), "# Root\n").unwrap();
        fs::create_dir_all(fixture.path().join("docs")).unwrap();
        fs::write(fixture.path().join("docs/CLAUDE.md"), "# Docs\n").unwrap();

        ensure_agent_memory_symlinks(fixture.path()).unwrap();

        assert_eq!(
            fs::read_link(fixture.path().join("AGENTS.md")).unwrap(),
            Path::new("CLAUDE.md")
        );
        assert_eq!(
            fs::read_link(fixture.path().join("GEMINI.md")).unwrap(),
            Path::new("CLAUDE.md")
        );
        assert_eq!(
            fs::read_link(fixture.path().join("docs/AGENTS.md")).unwrap(),
            Path::new("CLAUDE.md")
        );
    }
}
