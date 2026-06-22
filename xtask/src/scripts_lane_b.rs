//! Lane B Rust ports for release and plugin-layout shell scripts.
//!
//! The shell scripts remain as compatibility wrappers; these functions are the
//! canonical implementations.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::release_versions::{self, BumpLevel};
use crate::{scripts, scripts_lane_d};

pub fn bump_version(root: &Path, args: &[String]) -> Result<()> {
    let arg = args.first().map(String::as_str).unwrap_or("");
    let level = parse_legacy_bump_level(arg)?;
    release_versions::bump(root, "template", level)?;
    println!("Done. Review CHANGELOG.md before tagging.");
    Ok(())
}

pub fn check_version_sync(root: &Path, args: &[String]) -> Result<()> {
    if args.len() > 1 {
        bail!("Usage: scripts/check-version-sync.sh [PROJECT_DIR]");
    }
    let project_dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_owned());
    release_versions::check_version_sync(&project_dir)
}

pub fn pre_release_check(args: &[String]) -> Result<()> {
    let options = PreReleaseOptions::parse(args)?;
    let mut runner = CheckRunner::default();

    runner.run("PATTERNS.md contracts", "cargo", &["xtask", "patterns"]);
    runner.run("plugin layout", "just", &["validate-plugin"]);
    runner.run(
        "schema docs",
        "cargo",
        &["xtask", "check-schema-docs", "--check"],
    );
    runner.run(
        "OpenAPI docs",
        "cargo",
        &["xtask", "check-openapi", "--check"],
    );
    runner.run(
        "scaffold intent contract",
        "cargo",
        &["xtask", "check-scaffold-intent-contract"],
    );
    runner.run(
        "template feature smoke",
        "cargo",
        &["xtask", "test-template-features"],
    );
    runner.run(
        "release version gate",
        "cargo",
        &[
            "xtask",
            "check-release-versions",
            "--base",
            "origin/main",
            "--head",
            "HEAD",
            "--mode",
            "pr",
        ],
    );
    runner.run("blob size", "cargo", &["xtask", "check-blob-size"]);
    runner.run("ascii hygiene", "just", &["ascii-check"]);

    if options.run_verify {
        runner.run("quality gate", "just", &["verify"]);
    }
    if options.run_build_plugin {
        runner.run("plugin package validation", "just", &["build-plugin"]);
    }
    if options.run_mcporter {
        runner.run("mcporter integration", "just", &["test-mcporter"]);
    }

    runner.finish()
}

pub fn test_template_features(repo_root: &Path) -> Result<()> {
    let mut smoke = SmokeRunner::default();
    let temp_root = TempDir::create("rtemplate-feature-smoke")?;
    let repo = temp_root.path().join("repo");

    run_silent_in(repo_root, "git", &["init", "-q", path_str(&repo)?])
        .context("failed to initialize temporary git repo")?;

    run_silent_in(
        &repo,
        "git",
        &["config", "user.email", "test@example.invalid"],
    )?;
    run_silent_in(&repo, "git", &["config", "user.name", "Template Test"])?;
    std::fs::write(repo.join(".env.example"), "safe=true\n")?;
    std::fs::write(repo.join(".env"), "secret=true\n")?;
    run_silent_in(&repo, "git", &["add", ".env.example"])?;
    run_silent_in(&repo, "git", &["add", "-f", ".env"])?;

    smoke.expect_fail("env guard blocks staged .env", run_env_guard_in(&repo));

    run_silent_in(&repo, "git", &["reset", "-q", ".env"])?;
    smoke.expect_ok("env guard allows .env.example", run_env_guard_in(&repo));

    let nested = temp_root.path().join("docs/nested");
    std::fs::create_dir_all(&nested)?;
    std::fs::write(temp_root.path().join("CLAUDE.md"), "# Root\n")?;
    std::fs::write(nested.join("CLAUDE.md"), "# Nested\n")?;
    match create_agent_memory_symlinks(temp_root.path()) {
        Ok(())
            if temp_root.path().join("AGENTS.md").is_symlink()
                && temp_root.path().join("GEMINI.md").is_symlink()
                && nested.join("AGENTS.md").is_symlink()
                && nested.join("GEMINI.md").is_symlink() =>
        {
            smoke.pass("symlink-docs inline pattern creates AGENTS/GEMINI links");
        }
        _ => smoke.fail("symlink-docs inline pattern creates AGENTS/GEMINI links"),
    }

    smoke.expect_ok(
        "plugin layout validator passes",
        validate_plugin_layout(repo_root, None),
    );
    smoke.expect_ok(
        "schema docs checker passes",
        scripts_lane_d::check_schema_docs(&["--check".to_owned()]),
    );
    smoke.expect_ok(
        "ascii checker catches allowed repo glyphs cleanly",
        run_ascii_checker(repo_root),
    );

    smoke.finish()
}

pub fn validate_plugin_layout(repo_root: &Path, plugin_root: Option<&Path>) -> Result<()> {
    let plugin_root = plugin_root
        .map(PathBuf::from)
        .or_else(|| env_path("PLUGIN_ROOT"))
        .unwrap_or_else(|| PathBuf::from("plugins/rtemplate"));
    let plugin_root = repo_root.join(plugin_root);
    let layout = PluginLayout::new(&plugin_root);
    let mut checks = PluginChecks::default();

    println!("=== Validating rmcp-template Plugin Layout ===");
    println!("Plugin root: {}", display_relative(repo_root, &plugin_root));
    println!();

    checks.check("jq is available", || command_on_path("jq"));

    checks.check_result("Claude plugin manifest exists", || {
        file_exists(&layout.claude)
    });
    checks.check_result("Claude plugin manifest is valid JSON", || {
        read_json(&layout.claude).map(|_| ())
    });
    checks.check_result("Claude plugin name is rtemplate", || {
        json_field_eq(&layout.claude, "/name", "rtemplate")
    });
    checks.check_result("Claude plugin has no version field", || {
        json_has_no_version(&layout.claude)
    });
    checks.check_result("Claude plugin points to hooks config", || {
        json_field_eq(&layout.claude, "/hooks", "./hooks/hooks.json")
    });
    checks.check_result("Claude plugin points to skills directory", || {
        json_field_eq(&layout.claude, "/skills", "./skills")
    });
    checks.check_result(
        "Claude plugin declares optional server_url userConfig",
        || {
            let value = read_json(&layout.claude)?;
            require_json_bool(&value, "/userConfig/server_url/required", false)?;
            require_json_str(&value, "/userConfig/server_url/default", "")?;
            Ok(())
        },
    );
    checks.check_result("Claude plugin declares api_token as sensitive", || {
        let value = read_json(&layout.claude)?;
        require_json_bool(&value, "/userConfig/api_token/sensitive", true)
    });
    checks.check_result("Claude plugin declares no_auth toggle", || {
        json_field_eq(&layout.claude, "/userConfig/no_auth/type", "boolean")
    });
    checks.check_result("Claude plugin declares auth_mode default", || {
        json_field_eq(&layout.claude, "/userConfig/auth_mode/default", "bearer")
    });

    checks.check_result("Codex plugin manifest exists", || {
        file_exists(&layout.codex)
    });
    checks.check_result("Codex plugin manifest is valid JSON", || {
        read_json(&layout.codex).map(|_| ())
    });
    checks.check_result("Codex plugin name is rtemplate-mcp", || {
        json_field_eq(&layout.codex, "/name", "rtemplate-mcp")
    });
    checks.check_result("Codex plugin has no version field", || {
        json_has_no_version(&layout.codex)
    });
    checks.check_result("Codex plugin points to skills directory", || {
        json_field_eq(&layout.codex, "/skills", "./skills/")
    });

    checks.check_result("Gemini extension manifest exists", || {
        file_exists(&layout.gemini)
    });
    checks.check_result("Gemini extension manifest is valid JSON", || {
        read_json(&layout.gemini).map(|_| ())
    });
    checks.check_result("Gemini extension name is rtemplate-mcp", || {
        json_field_eq(&layout.gemini, "/name", "rtemplate-mcp")
    });
    checks.check_result("Gemini extension has no version field", || {
        json_has_no_version(&layout.gemini)
    });
    checks.check_result("Gemini extension points to skills directory", || {
        json_field_eq(&layout.gemini, "/skills", "./skills")
    });

    // Marketplace manifests intentionally do not bundle MCP server registration
    // (see plugins/README.md): the server connects through the user's gateway or
    // local MCP setup. No .mcp.json / mcpServers checks here by design.

    checks.check_result("hooks config exists", || file_exists(&layout.hooks));
    checks.check_result("hooks config is valid JSON", || {
        read_json(&layout.hooks).map(|_| ())
    });
    checks.check_result("SessionStart runs plugin setup", || {
        hook_command_exists(&layout.hooks, "SessionStart", None)
    });
    checks.check_result("ConfigChange runs plugin setup", || {
        hook_command_exists(&layout.hooks, "ConfigChange", Some("user_settings"))
    });

    checks.check_result("skills directory exists", || dir_exists(&layout.skills));

    let skill_files = skill_files(&layout.skills).unwrap_or_default();
    for skill_file in &skill_files {
        let skill_dir = skill_file
            .parent()
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .unwrap_or("<unknown>");
        checks.check_result(&format!("skill {skill_dir} has front matter name"), || {
            skill_has_name(skill_file, skill_dir)
        });
        checks.check_result(&format!("skill {skill_dir} has description"), || {
            skill_has_description(skill_file)
        });
    }

    checks.check("at least one plugin skill exists", || {
        !skill_files.is_empty()
    });
    checks.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreReleaseOptions {
    run_verify: bool,
    run_build_plugin: bool,
    run_mcporter: bool,
}

impl Default for PreReleaseOptions {
    fn default() -> Self {
        Self {
            run_verify: true,
            run_build_plugin: true,
            run_mcporter: false,
        }
    }
}

impl PreReleaseOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self::default();
        for arg in args {
            match arg.as_str() {
                "--skip-verify" => options.run_verify = false,
                "--skip-build-plugin" => options.run_build_plugin = false,
                "--mcporter" => options.run_mcporter = true,
                "-h" | "--help" => {
                    println!("{}", PRE_RELEASE_USAGE.trim_end());
                    std::process::exit(0);
                }
                unknown => {
                    eprintln!("unknown argument: {unknown}");
                    eprintln!("{}", PRE_RELEASE_USAGE.trim_end());
                    bail!("unknown argument: {unknown}");
                }
            }
        }
        Ok(options)
    }
}

const PRE_RELEASE_USAGE: &str = r#"Usage: scripts/pre-release-check.sh [OPTIONS]

Options:
  --skip-verify        Skip `just verify`.
  --skip-build-plugin  Skip `just build-plugin`.
  --mcporter           Also run `just test-mcporter` (requires running server).
  -h, --help           Show this help.
"#;

#[derive(Default)]
struct CheckRunner {
    pass: usize,
    fail: usize,
    failed_checks: Vec<String>,
}

impl CheckRunner {
    fn run(&mut self, label: &str, program: &str, args: &[&str]) {
        println!("\n==> {label}");
        match run_status(program, args, true) {
            Ok(status) if status.success() => {
                println!("PASS {label}");
                self.pass += 1;
            }
            _ => {
                eprintln!("FAIL {label}");
                self.failed_checks.push(label.to_owned());
                self.fail += 1;
            }
        }
    }

    fn finish(self) -> Result<()> {
        println!("\n== Results ==");
        println!("Passed: {}", self.pass);
        println!("Failed: {}", self.fail);
        if self.fail > 0 {
            eprintln!("Failed checks:");
            for label in &self.failed_checks {
                eprintln!("  - {label}");
            }
            bail!("pre-release check failed");
        }
        println!("Release gate passed.");
        Ok(())
    }
}

#[derive(Default)]
struct SmokeRunner {
    pass: usize,
    fail: usize,
}

impl SmokeRunner {
    fn pass(&mut self, label: &str) {
        println!("PASS  {label}");
        self.pass += 1;
    }

    fn fail(&mut self, label: &str) {
        eprintln!("FAIL  {label}");
        self.fail += 1;
    }

    fn expect_ok(&mut self, label: &str, result: Result<()>) {
        match result {
            Ok(()) => self.pass(label),
            Err(error) => {
                let output = error.to_string().replace('\n', "");
                let output: String = output.chars().take(200).collect();
                self.fail(&format!("{label} ({output})"));
            }
        }
    }

    fn expect_fail(&mut self, label: &str, result: Result<()>) {
        match result {
            Ok(()) => self.fail(&format!("{label} (unexpected success)")),
            Err(_) => self.pass(label),
        }
    }

    fn finish(self) -> Result<()> {
        println!("\n{} passed, {} failed", self.pass, self.fail);
        if self.fail == 0 {
            Ok(())
        } else {
            bail!("template feature smoke failed")
        }
    }
}

#[derive(Default)]
struct PluginChecks {
    checks: usize,
    passed: usize,
    failed: usize,
}

impl PluginChecks {
    fn check(&mut self, name: &str, test: impl FnOnce() -> bool) {
        self.check_result(name, || {
            if test() {
                Ok(())
            } else {
                bail!("check returned false")
            }
        });
    }

    fn check_result(&mut self, name: &str, test: impl FnOnce() -> Result<()>) {
        self.checks += 1;
        print!("Checking: {name}... ");
        if test().is_ok() {
            println!("\x1b[0;32mPASS\x1b[0m");
            self.passed += 1;
        } else {
            println!("\x1b[0;31mFAIL\x1b[0m");
            self.failed += 1;
        }
    }

    fn finish(self) -> Result<()> {
        println!();
        println!("=== Results ===");
        println!("Total checks: {}", self.checks);
        println!("\x1b[0;32mPassed: {}\x1b[0m", self.passed);
        if self.failed > 0 {
            println!("\x1b[0;31mFailed: {}\x1b[0m", self.failed);
            bail!("plugin layout validation failed");
        }
        println!("\x1b[0;32mAll checks passed.\x1b[0m");
        Ok(())
    }
}

struct PluginLayout {
    claude: PathBuf,
    codex: PathBuf,
    gemini: PathBuf,
    hooks: PathBuf,
    skills: PathBuf,
}

impl PluginLayout {
    fn new(plugin_root: &Path) -> Self {
        Self {
            claude: plugin_root.join(".claude-plugin/plugin.json"),
            codex: plugin_root.join(".codex-plugin/plugin.json"),
            gemini: plugin_root.join("gemini-extension.json"),
            hooks: plugin_root.join("hooks/hooks.json"),
            skills: plugin_root.join("skills"),
        }
    }
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn create(prefix: &str) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before UNIX_EPOCH")?
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn parse_legacy_bump_level(arg: &str) -> Result<BumpLevel> {
    match arg {
        "patch" => Ok(BumpLevel::Patch),
        "minor" => Ok(BumpLevel::Minor),
        "major" => Ok(BumpLevel::Major),
        "" => bail!("Usage: scripts/bump-version.sh <major|minor|patch>"),
        _ => bail!(
            "scripts/bump-version.sh now accepts only major, minor, or patch; use cargo xtask for component-aware bumps."
        ),
    }
}

fn file_exists(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("missing file {}", path.display())
    }
}

fn dir_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        bail!("missing directory {}", path.display())
    }
}

fn read_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn json_field_eq(path: &Path, pointer: &str, expected: &str) -> Result<()> {
    let value = read_json(path)?;
    require_json_str(&value, pointer, expected)
}

fn json_has_no_version(path: &Path) -> Result<()> {
    let value = read_json(path)?;
    if contains_json_key(&value, "version") {
        bail!("must not contain a version key")
    } else {
        Ok(())
    }
}

fn require_json_str(value: &Value, pointer: &str, expected: &str) -> Result<()> {
    let actual = value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("missing JSON string at {pointer}"))?;
    if actual == expected {
        Ok(())
    } else {
        bail!("expected {pointer} to be {expected:?}, found {actual:?}")
    }
}

fn require_json_bool(value: &Value, pointer: &str, expected: bool) -> Result<()> {
    let actual = value
        .pointer(pointer)
        .and_then(Value::as_bool)
        .with_context(|| format!("missing JSON boolean at {pointer}"))?;
    if actual == expected {
        Ok(())
    } else {
        bail!("expected {pointer} to be {expected}, found {actual}")
    }
}

fn contains_json_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key(key) || map.values().any(|value| contains_json_key(value, key))
        }
        Value::Array(values) => values.iter().any(|value| contains_json_key(value, key)),
        _ => false,
    }
}

fn hook_command_exists(path: &Path, event: &str, matcher: Option<&str>) -> Result<()> {
    let value = read_json(path)?;
    let entries = value
        .pointer(&format!("/hooks/{event}"))
        .and_then(Value::as_array)
        .with_context(|| format!("missing hooks.{event} array"))?;
    let found = entries.iter().any(|entry| {
        if matcher
            .is_some_and(|expected| entry.get("matcher").and_then(Value::as_str) != Some(expected))
        {
            return false;
        }
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command").and_then(Value::as_str)
                        == Some("rtemplate setup plugin-hook")
                })
            })
    });
    if found {
        Ok(())
    } else {
        bail!("missing {event} hook command")
    }
}

fn skill_files(skills_dir: &Path) -> Result<Vec<PathBuf>> {
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(skills_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let skill = entry.path().join("SKILL.md");
        if skill.is_file() {
            files.push(skill);
        }
    }
    files.sort();
    Ok(files)
}

fn skill_has_name(skill_file: &Path, expected: &str) -> Result<()> {
    let content = std::fs::read_to_string(skill_file)
        .with_context(|| format!("failed to read {}", skill_file.display()))?;
    let expected_line = format!("name: {expected}");
    if content.lines().any(|line| line.trim_end() == expected_line) {
        Ok(())
    } else {
        bail!("missing front matter name {expected}")
    }
}

fn skill_has_description(skill_file: &Path) -> Result<()> {
    let content = std::fs::read_to_string(skill_file)
        .with_context(|| format!("failed to read {}", skill_file.display()))?;
    if content.lines().any(|line| {
        line.strip_prefix("description:")
            .is_some_and(|value| !value.trim().is_empty())
    }) {
        Ok(())
    } else {
        bail!("missing description")
    }
}

fn create_agent_memory_symlinks(root: &Path) -> Result<()> {
    for claude in find_named_files(root, "CLAUDE.md")? {
        let dir = claude
            .parent()
            .with_context(|| format!("{} has no parent", claude.display()))?;
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

fn find_named_files(root: &Path, name: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_dirs(root, &mut |path| {
        if path.file_name().and_then(OsStr::to_str) == Some(name) {
            files.push(path.to_owned());
        }
        Ok(())
    })?;
    Ok(files)
}

fn visit_dirs(dir: &Path, f: &mut impl FnMut(&Path) -> Result<()>) -> Result<()> {
    if dir
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| matches!(name, ".git" | "target"))
    {
        return Ok(());
    }
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            visit_dirs(&path, f)?;
        } else if file_type.is_file() {
            f(&path)?;
        }
    }
    Ok(())
}

fn run_ascii_checker(repo_root: &Path) -> Result<()> {
    let output = run_output_in(
        repo_root,
        "git",
        &[
            "ls-files",
            "*.md",
            "*.rs",
            "*.toml",
            "*.json",
            "*.yml",
            "*.yaml",
            "*.sh",
            "*.py",
            ":!:docs/references/**",
            ":!:docs/sessions/**",
        ],
    )?;
    let mut files: Vec<String> = output
        .lines()
        .filter(|path| repo_root.join(path).is_file())
        .map(ToOwned::to_owned)
        .collect();
    let mut args = Vec::new();
    args.append(&mut files);
    let original = std::env::current_dir().context("failed to capture current directory")?;
    std::env::set_current_dir(repo_root)
        .with_context(|| format!("failed to enter {}", repo_root.display()))?;
    let result = scripts_lane_d::asciicheck(&args);
    let restore = std::env::set_current_dir(&original)
        .with_context(|| format!("failed to restore {}", original.display()));
    restore?;
    result
}

fn run_silent_in(cwd: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = run_status_in(cwd, program, args, false)?;
    if status.success() {
        Ok(())
    } else {
        bail!("{program} exited with {status}")
    }
}

fn run_env_guard_in(cwd: &Path) -> Result<()> {
    let original = std::env::current_dir().context("failed to capture current directory")?;
    std::env::set_current_dir(cwd).with_context(|| format!("failed to enter {}", cwd.display()))?;
    let result = scripts::block_env_commits();
    let restore = std::env::set_current_dir(&original)
        .with_context(|| format!("failed to restore {}", original.display()));
    restore?;
    result
}

fn run_status(program: &str, args: &[&str], inherit: bool) -> Result<std::process::ExitStatus> {
    run_status_in(Path::new("."), program, args, inherit)
}

fn run_status_in(
    cwd: &Path,
    program: &str,
    args: &[&str],
    inherit: bool,
) -> Result<std::process::ExitStatus> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd).stdin(Stdio::null());
    if inherit {
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    command
        .status()
        .with_context(|| format!("failed to spawn {program}"))
}

fn run_output_in(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to spawn {program}"))?;
    if !output.status.success() {
        bail!("{program} exited with {}", output.status);
    }
    String::from_utf8(output.stdout).with_context(|| format!("{program} emitted non-UTF-8 stdout"))
}

fn command_on_path(name: &str) -> bool {
    if name.contains('/') {
        return Path::new(name).is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("non-UTF-8 path: {}", path.display()))
}

fn display_relative<'a>(root: &'a Path, path: &'a Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        contains_json_key, json_has_no_version, parse_legacy_bump_level, skill_has_description,
        skill_has_name, PreReleaseOptions,
    };
    use crate::release_versions::BumpLevel;
    use serde_json::json;

    #[test]
    fn legacy_bump_parser_matches_wrapper_contract() {
        assert_eq!(parse_legacy_bump_level("patch").unwrap(), BumpLevel::Patch);
        assert_eq!(parse_legacy_bump_level("minor").unwrap(), BumpLevel::Minor);
        assert_eq!(parse_legacy_bump_level("major").unwrap(), BumpLevel::Major);
        assert!(parse_legacy_bump_level("")
            .unwrap_err()
            .to_string()
            .contains("Usage:"));
        assert!(parse_legacy_bump_level("template")
            .unwrap_err()
            .to_string()
            .contains("component-aware bumps"));
    }

    #[test]
    fn pre_release_options_preserve_shell_defaults() {
        let options = PreReleaseOptions::parse(&[]).unwrap();
        assert!(options.run_verify);
        assert!(options.run_build_plugin);
        assert!(!options.run_mcporter);

        let options = PreReleaseOptions::parse(&[
            "--skip-verify".to_owned(),
            "--skip-build-plugin".to_owned(),
            "--mcporter".to_owned(),
        ])
        .unwrap();
        assert!(!options.run_verify);
        assert!(!options.run_build_plugin);
        assert!(options.run_mcporter);
    }

    #[test]
    fn version_key_check_is_recursive() {
        assert!(contains_json_key(
            &json!({"nested": [{"version": "1"}]}),
            "version"
        ));
        assert!(!contains_json_key(&json!({"name": "rtemplate"}), "version"));
    }

    #[test]
    fn json_no_version_rejects_nested_version_key() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), r#"{"name":"x","nested":{"version":"1"}}"#).unwrap();
        assert!(json_has_no_version(temp.path()).is_err());
    }

    #[test]
    fn skill_front_matter_checks_match_shell_awk_rules() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "name: rtemplate\ndescription: Useful skill\n").unwrap();
        assert!(skill_has_name(temp.path(), "rtemplate").is_ok());
        assert!(skill_has_description(temp.path()).is_ok());

        std::fs::write(temp.path(), "name: other\ndescription:\n").unwrap();
        assert!(skill_has_name(temp.path(), "rtemplate").is_err());
        assert!(skill_has_description(temp.path()).is_err());
    }
}
