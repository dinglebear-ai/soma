use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::{cargo_generate_post, command_exists};

#[derive(Debug)]
struct Case {
    name: &'static str,
    values: BTreeMap<&'static str, &'static str>,
    feature_checks: &'static [&'static str],
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    let mut cargo_check = true;
    for arg in args {
        match arg.as_str() {
            "--no-cargo-check" => cargo_check = false,
            "--help" | "-h" => {
                println!("Usage: cargo xtask cargo-generate [--no-cargo-check]");
                return Ok(());
            }
            unknown => bail!("Unknown cargo-generate option: {unknown}"),
        }
    }

    if !command_exists("cargo-generate") {
        bail!("cargo-generate is not installed; run `cargo install cargo-generate`");
    }

    let repo = std::env::current_dir().context("failed to read current directory")?;
    let temp = TempDir::new("rtemplate-cargo-generate")?;
    let template = temp.path().join("_template");
    let cargo_home = stage_cargo_home(temp.path())?;
    stage_template(&repo, &template)?;

    for case in cases() {
        println!("== {} ==", case.name);
        generate_case(&template, temp.path(), &cargo_home, &case, cargo_check)?;
    }

    Ok(())
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "simple",
            values: BTreeMap::from([
                ("package_name", "myservice-mcp"),
                ("crate_prefix", "myservice"),
                ("binary_name", "myservice"),
                ("server_binary_name", "myservice-server"),
                ("service_slug", "myservice"),
                ("type_prefix", "MyService"),
                ("env_prefix", "MYSERVICE"),
                ("scope_prefix", "myservice"),
                ("default_port", "40123"),
                ("github_owner", "jmagar"),
                ("github_repo", "myservice-mcp"),
                ("default_features", "full"),
            ]),
            feature_checks: &[
                "cli",
                "mcp-stdio",
                "local-adapter",
                "api,cli,web,oauth,observability",
                "server",
                "full",
            ],
        },
        Case {
            name: "hyphenated-packages",
            values: BTreeMap::from([
                ("package_name", "foo-bar-mcp"),
                ("crate_prefix", "foo-bar"),
                ("binary_name", "foo-bar"),
                ("server_binary_name", "foo-bar-server"),
                ("service_slug", "foo_bar"),
                ("type_prefix", "FooBar"),
                ("env_prefix", "FOOBAR"),
                ("scope_prefix", "foo-bar"),
                ("default_port", "40124"),
                ("github_owner", "jmagar"),
                ("github_repo", "foo-bar-mcp"),
                ("default_features", "server,web,oauth,observability"),
            ]),
            feature_checks: &[
                "cli",
                "mcp-stdio",
                "local-adapter",
                "api,cli,web,oauth,observability",
                "server,web,oauth,observability",
            ],
        },
        Case {
            name: "upstream-client-local-adapter",
            values: BTreeMap::from([
                ("package_name", "lean-mcp"),
                ("crate_prefix", "lean"),
                ("binary_name", "lean"),
                ("server_binary_name", "lean-server"),
                ("service_slug", "lean"),
                ("type_prefix", "Lean"),
                ("env_prefix", "LEAN"),
                ("scope_prefix", "lean"),
                ("default_port", "40090"),
                ("github_owner", "jmagar"),
                ("github_repo", "lean-mcp"),
                ("default_features", "local-adapter"),
            ]),
            feature_checks: &["local-adapter", "cli", "mcp-stdio"],
        },
    ]
}

pub(crate) fn stage_template(repo: &Path, template: &Path) -> Result<()> {
    for entry in WalkDir::new(repo).into_iter().filter_entry(|entry| {
        let relative = match entry.path().strip_prefix(repo) {
            Ok(path) => path,
            Err(_) => return true,
        };
        !is_ignored(relative)
    }) {
        let entry = entry.context("failed to walk template source")?;
        let relative = entry
            .path()
            .strip_prefix(repo)
            .context("walk entry was outside repo")?;
        if relative.as_os_str().is_empty() {
            continue;
        }

        let destination = template.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)
                .with_context(|| format!("failed to create {}", destination.display()))?;
        } else if entry.file_type().is_file() || entry.file_type().is_symlink() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(entry.path(), &destination).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }

    let root_bin = template.join("bin");
    if root_bin.exists() {
        fs::remove_dir_all(&root_bin)
            .with_context(|| format!("failed to remove {}", root_bin.display()))?;
    }

    Ok(())
}

fn is_ignored(relative: &Path) -> bool {
    const IGNORED_NAMES: &[&str] = &[
        ".beads",
        ".cache",
        ".dolt",
        ".full-review",
        ".git",
        ".lavra",
        ".next",
        ".serena",
        ".superpowers",
        ".worktrees",
        "__pycache__",
        "node_modules",
        "target",
        "dist",
        "storage",
    ];
    const IGNORED_PATHS: &[&str] = &["docs/references", "mcp-server-inventory.md"];

    let normalized = relative.to_string_lossy().replace('\\', "/");
    if IGNORED_PATHS.iter().any(|path| normalized == *path) {
        return true;
    }
    if normalized.ends_with(".pyc") {
        return true;
    }
    relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        IGNORED_NAMES.iter().any(|ignored| name == *ignored)
    })
}

fn generate_case(
    template: &Path,
    temp: &Path,
    cargo_home: &Path,
    case: &Case,
    cargo_check: bool,
) -> Result<()> {
    let destination = temp.join(case.name);
    fs::create_dir_all(&destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    let mut args = vec![
        "generate".to_string(),
        "--path".to_string(),
        template.display().to_string(),
        "--name".to_string(),
        value(case, "package_name")?.to_string(),
        "--destination".to_string(),
        destination.display().to_string(),
        "--vcs".to_string(),
        "none".to_string(),
        "--silent".to_string(),
    ];
    for (key, value) in &case.values {
        args.push("--define".to_string());
        args.push(format!("{key}={value}"));
    }

    let cargo_args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cmd_in("cargo", &cargo_args, Path::new("."), cargo_home)?;

    let project = destination.join(value(case, "package_name")?);
    cargo_generate_post::run(&[project.display().to_string()])?;
    assert_generated_shape(&project, case)?;

    if cargo_check {
        run_cmd_in(
            "cargo",
            &["check", "--workspace", "--all-targets"],
            &project,
            cargo_home,
        )
        .with_context(|| format!("cargo check failed in {}", project.display()))?;

        for features in case.feature_checks {
            run_cmd_in(
                "cargo",
                &[
                    "check",
                    "-p",
                    value(case, "package_name")?,
                    "--no-default-features",
                    "--features",
                    features,
                ],
                &project,
                cargo_home,
            )
            .with_context(|| {
                format!(
                    "cargo check failed in {} for features {features}",
                    project.display()
                )
            })?;
        }
    }

    Ok(())
}

fn assert_generated_shape(project: &Path, case: &Case) -> Result<()> {
    let surface_name = format!("{}-mcp-surface", value(case, "crate_prefix")?);

    assert_missing(project.join("cargo-generate.toml"))?;
    assert_missing(project.join(".cargo-generate-values.toml"))?;
    assert_missing(project.join("template"))?;
    assert_missing(project.join("docs/CARGO_GENERATE.md"))?;

    let readme = read_to_string(project.join("README.md"))?;
    if readme.contains("Generate a New Server") {
        bail!("generated README still contains template generation instructions");
    }
    if readme.contains(&format!(
        "https://github.com/{}/{}",
        value(case, "github_owner")?,
        surface_name
    )) {
        bail!("generated README points at the internal MCP surface crate repo");
    }

    let package_crate = format!("crates/{}", value(case, "package_name")?);
    let manifest = read_to_string(project.join(&package_crate).join("Cargo.toml"))?;
    let expected_default = format!(
        "default = [{}]",
        value(case, "default_features")?
            .split(',')
            .filter(|feature| !feature.trim().is_empty())
            .map(|feature| format!("\"{}\"", feature.trim()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !manifest.contains(&expected_default) {
        bail!("generated Cargo.toml does not contain {expected_default}");
    }

    let web_crate = format!("crates/{}-web", value(case, "crate_prefix")?);
    for bundled_source in [
        "assets/source/package.json",
        "assets/source/components/aurora.css",
        "assets/source/app/page.tsx",
    ] {
        assert_exists(project.join(&web_crate).join(bundled_source))?;
    }
    for generated_artifact in [
        "assets/source/node_modules",
        "assets/source/.next",
        "assets/source/out",
        "assets/source/tsconfig.tsbuildinfo",
    ] {
        assert_missing(project.join(&web_crate).join(generated_artifact))?;
    }

    let expected_repo = format!(
        "https://github.com/{}/{}",
        value(case, "github_owner")?,
        value(case, "github_repo")?
    );
    for plugin_file in [
        project.join(format!(
            "plugins/{}/.claude-plugin/plugin.json",
            value(case, "binary_name")?
        )),
        project.join(format!(
            "plugins/{}/.codex-plugin/plugin.json",
            value(case, "binary_name")?
        )),
        project.join(format!(
            "plugins/{}/gemini-extension.json",
            value(case, "binary_name")?
        )),
    ] {
        let body = read_to_string(&plugin_file)?;
        if body.contains(&surface_name) {
            bail!(
                "{} leaks internal crate name {surface_name:?}",
                plugin_file.display()
            );
        }
        if !body.contains(&expected_repo) {
            bail!("{} does not contain {expected_repo}", plugin_file.display());
        }
    }

    Ok(())
}

fn assert_exists(path: PathBuf) -> Result<()> {
    if !path.exists() {
        bail!("generated project is missing {}", path.display());
    }
    Ok(())
}

fn assert_missing(path: PathBuf) -> Result<()> {
    if path.exists() {
        bail!("generated project still contains {}", path.display());
    }
    Ok(())
}

fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn value<'a>(case: &'a Case, key: &str) -> Result<&'a str> {
    case.values
        .get(key)
        .copied()
        .with_context(|| format!("case {} is missing {key}", case.name))
}

fn stage_cargo_home(temp: &Path) -> Result<PathBuf> {
    let cargo_home = temp.join("cargo-home");
    fs::create_dir_all(&cargo_home)
        .with_context(|| format!("failed to create {}", cargo_home.display()))?;

    let source_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    let Some(source_home) = source_home else {
        return Ok(cargo_home);
    };

    for cache_name in ["registry", "git"] {
        let source = source_home.join(cache_name);
        let destination = cargo_home.join(cache_name);
        if source.exists() {
            symlink_dir(&source, &destination).with_context(|| {
                format!(
                    "failed to link Cargo cache {} to {}",
                    source.display(),
                    destination.display()
                )
            })?;
        }
    }

    Ok(cargo_home)
}

#[cfg(unix)]
fn symlink_dir(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, destination)
}

#[cfg(windows)]
fn symlink_dir(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, destination)
}

fn run_cmd_in(program: &str, args: &[&str], cwd: &Path, cargo_home: &Path) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd).stdin(Stdio::null());
    if program == "cargo" {
        command.env("CARGO_HOME", cargo_home);
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

pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new(prefix: &str) -> Result<Self> {
        let mut path = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system time is before Unix epoch")?
            .as_nanos();
        path.push(format!("{prefix}-{}-{now}", std::process::id()));
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create temp dir {}", path.display()))?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
