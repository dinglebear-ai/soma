//! Lane C ports for broad ops scripts.
//!
//! These functions are intentionally not wired into `main.rs` in this lane.
//! The parent integration lane can expose them as xtask commands after checking
//! command names and wrapper behavior.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DEFAULT_MAX_BYTES: u64 = 500 * 1024;
const DEFAULT_BLOB_ALLOWLIST: &str = "scripts/blob-size-allowlist.txt";
const DEFAULT_RUNTIME_UNIT: &str = "soma-mcp.service";
const DEFAULT_RUNTIME_SERVICE: &str = "soma-mcp";
const REQUIRED_PLUGIN_FIELDS: [&str; 5] = [
    "exit_policy",
    "ran_repair",
    "no_repair",
    "blocking_failures",
    "advisory_failures",
];
const EXIT_POLICIES: [&str; 3] = ["success", "advisory_failure", "blocking_failure"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedBlob {
    pub path: String,
    pub size_bytes: u64,
    pub is_allowlisted: bool,
    pub is_binary: bool,
}

pub fn check_blob_size(args: &[String]) -> Result<()> {
    let options = BlobSizeOptions::parse(args)?;
    let allowlist = load_allowlist(&options.allowlist)?;
    let blobs = collect_changed_blobs(&options.base, &options.head, &allowlist)?;
    let violations: Vec<&ChangedBlob> = blobs
        .iter()
        .filter(|blob| blob.size_bytes > options.max_bytes && !blob.is_allowlisted)
        .collect();

    write_blob_step_summary(options.max_bytes, &blobs, &violations)?;

    if blobs.is_empty() {
        println!("No changed files were detected.");
        return Ok(());
    }

    println!(
        "Checked {} changed file(s) against the {}-byte limit.",
        blobs.len(),
        options.max_bytes
    );
    for blob in &blobs {
        let mut status = if blob.is_allowlisted {
            "allowlisted"
        } else {
            "ok"
        };
        if violations
            .iter()
            .any(|violation| violation.path == blob.path)
        {
            status = "blocked";
        }
        let kind = if blob.is_binary {
            "binary"
        } else {
            "non-binary"
        };
        println!(
            "- {}: {} bytes ({}) [{kind}, {status}]",
            blob.path,
            blob.size_bytes,
            format_kib(blob.size_bytes)
        );
    }

    if !violations.is_empty() {
        println!("\nFile(s) exceed the configured limit:");
        for blob in violations {
            println!(
                "- {}: {} bytes > {} bytes",
                blob.path, blob.size_bytes, options.max_bytes
            );
        }
        println!(
            "\nIf this is a real checked-in asset, add its repo-relative path or glob \
to scripts/blob-size-allowlist.txt. Otherwise, shrink it or keep it out of git."
        );
        bail!("changed blob(s) exceed the configured size limit");
    }

    Ok(())
}

pub fn check_dependency_updates(args: &[String]) -> Result<()> {
    let options = DependencyUpdateOptions::parse(args)?;

    require_command("cargo")?;
    println!("\n== Lockfile-compatible updates ==");
    let dry_run = command_output(
        Command::new("cargo")
            .arg("update")
            .arg("--dry-run")
            .env("CARGO_TERM_COLOR", "never"),
    )
    .context("cargo update --dry-run failed")?;
    print!("{dry_run}");

    let mut updates_found = dry_run_reports_updates(&dry_run);

    if !options.skip_search {
        println!("\n== Direct dependency latest versions ==");
        println!(
            "{:<32} {:<18} {:<18} status",
            "crate", "requirement", "latest"
        );
        for dependency in extract_direct_deps(Path::new("Cargo.toml"))? {
            let version_req = dependency_version_req(&dependency.line);
            let Some(version_req) = version_req else {
                println!("{:<32} {:<18} {:<18} skipped", dependency.name, "-", "-");
                continue;
            };
            let latest = latest_crate_version(&dependency.name)?;
            let Some(latest) = latest else {
                println!(
                    "{:<32} {:<18} {:<18} check failed",
                    dependency.name, version_req, "unknown"
                );
                continue;
            };
            let status = direct_dependency_status(&version_req, &latest);
            if status == "review" {
                updates_found = true;
            }
            println!(
                "{:<32} {:<18} {:<18} {status}",
                dependency.name, version_req, latest
            );
        }
    }

    println!("\n== Result ==");
    if updates_found {
        println!("Dependency updates may be available.");
        if options.fail_on_updates {
            bail!("dependency updates may be available");
        }
    } else {
        println!("No dependency updates detected.");
    }

    Ok(())
}

pub fn check_runtime_current(args: &[String]) -> Result<()> {
    let mut options = RuntimeOptions::from_env();
    options.parse_args(args)?;

    let mode = if options.mode == RuntimeMode::Auto {
        detect_runtime_mode(&options)?
    } else {
        options.mode
    };

    match mode {
        RuntimeMode::Systemd => check_runtime_systemd(&options),
        RuntimeMode::Docker => check_runtime_docker(&options),
        RuntimeMode::Auto => unreachable!("auto mode should have been resolved"),
        RuntimeMode::None => {
            bail!(
                "FAIL: no running {} systemd unit or {} container detected",
                options.unit,
                options.service
            )
        }
    }
}

pub fn check_plugin_hook_contract(args: &[String]) -> Result<()> {
    let execute = match args {
        [] => false,
        [arg] if arg == "--execute" => true,
        [arg] if arg == "--help" || arg == "-h" => {
            println!(
                "Usage: cargo xtask check-plugin-hook-contract [--execute]\n\
                 \n\
                 Audit binary-owned plugin hook setup contracts across Rust MCP servers."
            );
            return Ok(());
        }
        _ => bail!("Usage: cargo xtask check-plugin-hook-contract [--execute]"),
    };

    for server in default_plugin_servers()? {
        check_plugin_layout(&server)?;
        check_required_recipes(&server)?;
        check_hook_delegation(&server)?;
        if execute {
            check_plugin_binary_contract(&server)?;
        }
        println!("ok {}", server.name);
    }

    Ok(())
}

pub fn refresh_docs(args: &[String]) -> Result<()> {
    let options = RefreshDocsOptions::parse(args)?;
    if options.skip_crawl && options.skip_repomix {
        bail!("ERROR: --skip-crawl and --skip-repomix cannot both be set");
    }

    let root = std::env::current_dir().context("failed to read current directory")?;
    let ref_dir = root.join("docs/references");
    let changes_file = ref_dir.join("CHANGES.md");
    let axon_output_dir = env_path("AXON_OUTPUT_DIR")
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".axon/output")))
        .unwrap_or_else(|| PathBuf::from(".axon/output"));

    let mut before_snapshot = Vec::new();
    if !options.dry_run {
        before_snapshot = snapshot_references(&ref_dir)?;
    }

    fs::create_dir_all(ref_dir.join("mcp/docs"))?;
    fs::create_dir_all(ref_dir.join("mcp/repos"))?;
    fs::create_dir_all(ref_dir.join("claude-code"))?;
    fs::create_dir_all(ref_dir.join("mcporter/docs"))?;
    fs::create_dir_all(ref_dir.join("mcporter/repos"))?;

    if !options.skip_crawl {
        let mut failed = false;
        for crawl in default_crawls() {
            if let Err(error) = crawl_docs(&options, &ref_dir, &axon_output_dir, crawl) {
                eprintln!(
                    "[refresh-docs] ERROR: {} docs crawl failed: {error}",
                    crawl.label
                );
                failed = true;
            }
        }
        if failed {
            bail!("ERROR: one or more required crawls failed - reference docs may be stale");
        }
    }

    if !options.skip_repomix {
        for pack in default_repomix_packs() {
            pack_repo(&options, &ref_dir, pack)?;
        }
        sparse_clone_path(
            &options,
            &ref_dir,
            SparseClone {
                remote: "https://github.com/openclaw/mcporter",
                sparse_path: "docs",
                target_rel: "mcporter/docs",
                mode: SparseCloneMode::Recursive,
            },
        )?;
    }

    if !options.dry_run {
        write_reference_index(&ref_dir)?;
        let after_snapshot = snapshot_references(&ref_dir)?;
        summarize_reference_changes(
            &changes_file,
            refresh_scope(&options),
            &before_snapshot,
            &after_snapshot,
        )?;
    }

    println!("[refresh-docs] done");
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlobSizeOptions {
    base: String,
    head: String,
    max_bytes: u64,
    allowlist: PathBuf,
}

impl BlobSizeOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut base = None;
        let mut head = "HEAD".to_owned();
        let mut max_bytes = DEFAULT_MAX_BYTES;
        let mut allowlist = PathBuf::from(DEFAULT_BLOB_ALLOWLIST);
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--base" => {
                    index += 1;
                    base = Some(
                        args.get(index)
                            .context("--base requires a value")?
                            .to_owned(),
                    );
                }
                "--head" => {
                    index += 1;
                    head = args
                        .get(index)
                        .context("--head requires a value")?
                        .to_owned();
                }
                "--max-bytes" => {
                    index += 1;
                    max_bytes = args
                        .get(index)
                        .context("--max-bytes requires a value")?
                        .parse()
                        .context("--max-bytes must be an integer")?;
                }
                "--allowlist" => {
                    index += 1;
                    allowlist = args
                        .get(index)
                        .context("--allowlist requires a value")?
                        .into();
                }
                "--help" | "-h" => {
                    bail!("Usage: cargo xtask check-blob-size [--base REF] [--head REF] [--max-bytes N] [--allowlist PATH]");
                }
                unknown => bail!("unknown option: {unknown}"),
            }
            index += 1;
        }

        Ok(Self {
            base: base.unwrap_or_else(default_blob_base),
            head,
            max_bytes,
            allowlist,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencyUpdateOptions {
    skip_search: bool,
    fail_on_updates: bool,
}

impl DependencyUpdateOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut skip_search = false;
        let mut fail_on_updates = false;
        for arg in args {
            match arg.as_str() {
                "--skip-search" => skip_search = true,
                "--fail-on-updates" => fail_on_updates = true,
                "--help" | "-h" => bail!("Usage: cargo xtask check-dependency-updates [--skip-search] [--fail-on-updates]"),
                unknown => bail!("ERROR: unknown option: {unknown}"),
            }
        }
        Ok(Self {
            skip_search,
            fail_on_updates,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeMode {
    Auto,
    Systemd,
    Docker,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeOptions {
    mode: RuntimeMode,
    pull: bool,
    unit: String,
    service: String,
    compose_dir: PathBuf,
    expected_binary: Option<PathBuf>,
}

impl RuntimeOptions {
    fn from_env() -> Self {
        Self {
            mode: RuntimeMode::Auto,
            pull: false,
            unit: std::env::var("SOMA_MCP_SYSTEMD_UNIT")
                .unwrap_or_else(|_| DEFAULT_RUNTIME_UNIT.to_owned()),
            service: std::env::var("SOMA_MCP_DOCKER_SERVICE")
                .unwrap_or_else(|_| DEFAULT_RUNTIME_SERVICE.to_owned()),
            compose_dir: env_path("SOMA_MCP_COMPOSE_DIR").unwrap_or_else(current_dir),
            expected_binary: env_path("SOMA_MCP_EXPECTED_BINARY"),
        }
    }

    fn parse_args(&mut self, args: &[String]) -> Result<()> {
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--mode" => {
                    index += 1;
                    self.mode = parse_runtime_mode(args.get(index).context("--mode requires a value")?)?;
                }
                "--pull" => self.pull = true,
                "--unit" => {
                    index += 1;
                    self.unit = args.get(index).context("--unit requires a value")?.to_owned();
                }
                "--service" => {
                    index += 1;
                    self.service = args
                        .get(index)
                        .context("--service requires a value")?
                        .to_owned();
                }
                "--compose-dir" => {
                    index += 1;
                    self.compose_dir = args
                        .get(index)
                        .context("--compose-dir requires a value")?
                        .into();
                }
                "--expected-binary" => {
                    index += 1;
                    self.expected_binary = Some(
                        args.get(index)
                            .context("--expected-binary requires a value")?
                            .into(),
                    );
                }
                "--help" | "-h" => bail!("Usage: cargo xtask check-runtime-current [--mode auto|systemd|docker] [--pull] [--unit NAME] [--service NAME] [--compose-dir DIR] [--expected-binary PATH]"),
                unknown => bail!("unknown argument: {unknown}"),
            }
            index += 1;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginServer {
    name: String,
    repo: PathBuf,
    binary: String,
    hook: Option<PathBuf>,
    plugin_root: Option<PathBuf>,
    check_plugin_layout: bool,
    package_args: Vec<String>,
    setup_args: Vec<String>,
    env: Vec<(String, String)>,
    appdata_env: String,
    make_appdata: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirectDependency {
    name: String,
    line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RefreshDocsOptions {
    dry_run: bool,
    skip_crawl: bool,
    skip_repomix: bool,
}

impl RefreshDocsOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut dry_run = false;
        let mut skip_crawl = false;
        let mut skip_repomix = false;
        for arg in args {
            match arg.as_str() {
                "--dry-run" => dry_run = true,
                "--skip-crawl" => skip_crawl = true,
                "--skip-repomix" => skip_repomix = true,
                "--help" | "-h" => bail!(
                    "Usage: cargo xtask refresh-docs [--dry-run] [--skip-crawl] [--skip-repomix]"
                ),
                unknown => bail!("ERROR: unknown option: {unknown}"),
            }
        }
        Ok(Self {
            dry_run,
            skip_crawl,
            skip_repomix,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct CrawlTarget {
    label: &'static str,
    url: &'static str,
    domain: &'static str,
    target_rel: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct RepoPack {
    remote: &'static str,
    target_rel: &'static str,
    include: &'static str,
    ignore: &'static str,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum SparseCloneMode {
    Recursive,
    FlatMdx,
}

#[derive(Debug, Clone, Copy)]
struct SparseClone {
    remote: &'static str,
    sparse_path: &'static str,
    target_rel: &'static str,
    mode: SparseCloneMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReferenceSnapshotEntry {
    path: String,
    sha256: String,
}

fn default_blob_base() -> String {
    for candidate in ["origin/main", "main"] {
        if git_status(["rev-parse", "--verify", candidate]) {
            return candidate.to_owned();
        }
    }
    "HEAD~1".to_owned()
}

fn load_allowlist(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read allowlist {}", path.display()))?;
    Ok(parse_allowlist(&text))
}

fn parse_allowlist(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            line.split_once('#')
                .map_or(Some(line), |(prefix, _)| Some(prefix))
        })
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn collect_changed_blobs(base: &str, head: &str, allowlist: &[String]) -> Result<Vec<ChangedBlob>> {
    let paths = get_changed_paths(base, head)?;
    let mut blobs = Vec::new();
    for path in paths {
        blobs.push(ChangedBlob {
            size_bytes: blob_size(head, &path)?,
            is_allowlisted: is_allowlisted(&path, allowlist),
            is_binary: is_binary_change(base, head, &path)?,
            path,
        });
    }
    Ok(blobs)
}

fn get_changed_paths(base: &str, head: &str) -> Result<Vec<String>> {
    let output = git_output([
        "diff",
        "--name-only",
        "--diff-filter=AM",
        "--no-renames",
        "-z",
        base,
        head,
    ])?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

fn is_binary_change(base: &str, head: &str, path: &str) -> Result<bool> {
    let output = git_output([
        "diff",
        "--numstat",
        "--diff-filter=AM",
        "--no-renames",
        base,
        head,
        "--",
        path,
    ])?;
    let Some(line) = output.lines().next() else {
        return Ok(false);
    };
    let fields: Vec<&str> = line.split('\t').collect();
    Ok(fields.len() >= 2 && fields[0] == "-" && fields[1] == "-")
}

fn blob_size(commit: &str, path: &str) -> Result<u64> {
    git_output(["cat-file", "-s", &format!("{commit}:{path}")])?
        .trim()
        .parse()
        .with_context(|| format!("failed to parse blob size for {commit}:{path}"))
}

fn is_allowlisted(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| glob_match(pattern, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return false;
    }
    if !path.starts_with(parts[0]) {
        return false;
    }
    if !pattern.ends_with('*') && !path.ends_with(parts.last().copied().unwrap_or_default()) {
        return false;
    }
    let mut remainder = path;
    for part in parts.into_iter().filter(|part| !part.is_empty()) {
        let Some(index) = remainder.find(part) else {
            return false;
        };
        remainder = &remainder[index + part.len()..];
    }
    true
}

fn format_kib(size_bytes: u64) -> String {
    format!("{:.1} KiB", size_bytes as f64 / 1024.0)
}

fn write_blob_step_summary(
    max_bytes: u64,
    blobs: &[ChangedBlob],
    violations: &[&ChangedBlob],
) -> Result<()> {
    let Some(summary_path) = env_path("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    let mut lines = vec![
        "## Blob Size Policy".to_owned(),
        String::new(),
        format!(
            "Default max: `{max_bytes}` bytes ({})",
            format_kib(max_bytes)
        ),
        format!("Changed files checked: `{}`", blobs.len()),
        format!("Violations: `{}`", violations.len()),
        String::new(),
    ];
    if blobs.is_empty() {
        lines.push("No changed files were detected.".to_owned());
    } else {
        lines.push("| Path | Kind | Size | Status |".to_owned());
        lines.push("| --- | --- | ---: | --- |".to_owned());
        for blob in blobs {
            let mut status = if blob.is_allowlisted {
                "allowlisted"
            } else {
                "ok"
            };
            if violations
                .iter()
                .any(|violation| violation.path == blob.path)
            {
                status = "blocked";
            }
            let kind = if blob.is_binary {
                "binary"
            } else {
                "non-binary"
            };
            lines.push(format!(
                "| `{}` | {kind} | `{}` bytes ({}) | {status} |",
                blob.path,
                blob.size_bytes,
                format_kib(blob.size_bytes)
            ));
        }
    }
    lines.push(String::new());
    fs::write(&summary_path, lines.join("\n"))
        .with_context(|| format!("failed to write {}", summary_path.display()))?;
    Ok(())
}

fn dry_run_reports_updates(output: &str) -> bool {
    output.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("Adding ")
            || trimmed.starts_with("Removing ")
            || trimmed.starts_with("Downgrading ")
            || (trimmed.starts_with("Updating ") && trimmed.contains(" v"))
            || (trimmed.starts_with("Locking ")
                && trimmed
                    .split_whitespace()
                    .nth(1)
                    .and_then(|value| value.parse::<usize>().ok())
                    .is_some_and(|count| count > 0))
    })
}

fn extract_direct_deps(manifest: &Path) -> Result<Vec<DirectDependency>> {
    let text = fs::read_to_string(manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    Ok(extract_direct_deps_from_toml(&text))
}

fn extract_direct_deps_from_toml(text: &str) -> Vec<DirectDependency> {
    let mut in_dependency_section = false;
    let mut dependencies = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_dependency_section = matches!(
                line,
                "[dependencies]"
                    | "[dev-dependencies]"
                    | "[build-dependencies]"
                    | "[workspace.dependencies]"
                    | "[workspace.dev-dependencies]"
                    | "[workspace.build-dependencies]"
            );
            continue;
        }
        if !in_dependency_section || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let name = line.split('=').next().map(str::trim).unwrap_or_default();
        if name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            dependencies.push(DirectDependency {
                name: name.to_owned(),
                line: line.to_owned(),
            });
        }
    }
    dependencies.sort_by(|left, right| left.line.cmp(&right.line));
    dependencies.dedup_by(|left, right| left.line == right.line);
    dependencies
}

fn dependency_version_req(line: &str) -> Option<String> {
    let rhs = line.split_once('=')?.1.trim();
    if let Some(value) = quoted_prefix(rhs) {
        return Some(value.to_owned());
    }
    let (_, after_version) = rhs.split_once("version")?;
    let (_, after_equals) = after_version.split_once('=')?;
    quoted_prefix(after_equals.trim()).map(str::to_owned)
}

fn quoted_prefix(value: &str) -> Option<&str> {
    let rest = value.strip_prefix('"')?;
    rest.split_once('"').map(|(version, _)| version)
}

fn latest_crate_version(crate_name: &str) -> Result<Option<String>> {
    let output = Command::new("cargo")
        .arg("search")
        .arg(crate_name)
        .arg("--limit")
        .arg("1")
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("failed to run cargo search {crate_name}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout =
        String::from_utf8(output.stdout).context("cargo search emitted non-UTF-8 stdout")?;
    Ok(parse_cargo_search_version(crate_name, &stdout))
}

fn parse_cargo_search_version(crate_name: &str, output: &str) -> Option<String> {
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some(crate_name) && parts.next() == Some("=") {
            return parts
                .next()
                .map(|version| version.trim_matches('"').to_owned());
        }
    }
    None
}

fn direct_dependency_status(requirement: &str, latest: &str) -> &'static str {
    if requirement == latest {
        "ok"
    } else if requirement_is_numeric_prefix(requirement)
        && latest.starts_with(&format!("{requirement}."))
    {
        "compatible-range"
    } else {
        "review"
    }
}

fn requirement_is_numeric_prefix(requirement: &str) -> bool {
    requirement.chars().all(|ch| ch.is_ascii_digit())
        || requirement.split_once('.').is_some_and(|(major, minor)| {
            !major.is_empty()
                && !minor.is_empty()
                && major.chars().all(|ch| ch.is_ascii_digit())
                && minor.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn parse_runtime_mode(value: &str) -> Result<RuntimeMode> {
    match value {
        "auto" => Ok(RuntimeMode::Auto),
        "systemd" => Ok(RuntimeMode::Systemd),
        "docker" => Ok(RuntimeMode::Docker),
        other => bail!("invalid mode: {other}"),
    }
}

fn detect_runtime_mode(options: &RuntimeOptions) -> Result<RuntimeMode> {
    if command_status(
        Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", &options.unit])
            .stderr(Stdio::null()),
    ) {
        return Ok(RuntimeMode::Systemd);
    }

    if command_exists("docker") {
        if options.compose_dir.is_dir() {
            let output = Command::new("docker")
                .args(["compose", "ps", "-q", &options.service])
                .current_dir(&options.compose_dir)
                .stderr(Stdio::null())
                .output();
            if output
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .is_some_and(|stdout| !stdout.trim().is_empty())
            {
                return Ok(RuntimeMode::Docker);
            }
        }
        let output = Command::new("docker")
            .args([
                "ps",
                "--filter",
                &format!("name=^/{}$", options.service),
                "--format",
                "{{.ID}}",
            ])
            .stderr(Stdio::null())
            .output();
        if output
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .is_some_and(|stdout| !stdout.trim().is_empty())
        {
            return Ok(RuntimeMode::Docker);
        }
    }

    Ok(RuntimeMode::None)
}

fn check_runtime_systemd(options: &RuntimeOptions) -> Result<()> {
    status_line("mode", "systemd");
    status_line("unit", &options.unit);
    let active = command_output(
        Command::new("systemctl")
            .args(["--user", "is-active", &options.unit])
            .stderr(Stdio::null()),
    )
    .unwrap_or_default();
    let active = active.trim();
    status_line("state", active);
    if active != "active" {
        bail!("FAIL: systemd unit is not active");
    }

    let pid = command_output(Command::new("systemctl").args([
        "--user",
        "show",
        &options.unit,
        "-p",
        "MainPID",
        "--value",
    ]))?;
    let pid = pid.trim();
    let proc_exe = PathBuf::from(format!("/proc/{pid}/exe"));
    if pid.is_empty() || pid == "0" || !proc_exe.exists() {
        bail!("FAIL: cannot resolve running process for {}", options.unit);
    }

    let exe = fs::canonicalize(&proc_exe)
        .with_context(|| format!("failed to resolve {}", proc_exe.display()))?;
    let exec_start = command_output(Command::new("systemctl").args([
        "--user",
        "show",
        &options.unit,
        "-p",
        "ExecStart",
        "--value",
    ]))?;
    let unit_exec = parse_systemd_exec_path(&exec_start)
        .ok_or_else(|| anyhow::anyhow!("FAIL: cannot parse ExecStart for {}", options.unit))?;
    let unit_exec = fs::canonicalize(unit_exec)
        .with_context(|| format!("failed to resolve unit ExecStart for {}", options.unit))?;

    let running_sha = sha256_file(&proc_exe)?;
    let unit_sha = sha256_file(&unit_exec)?;
    status_line("pid", pid);
    status_line("running_exe", &exe.display().to_string());
    status_line("unit_exec", &unit_exec.display().to_string());
    status_line("running_version", &version_of(&exe));
    status_line("unit_version", &version_of(&unit_exec));
    status_line("running_sha", &running_sha);
    status_line("unit_sha", &unit_sha);

    if running_sha != unit_sha {
        println!("STALE: running process does not match unit ExecStart binary");
        println!("fix: systemctl --user restart {}", options.unit);
        bail!("running systemd process is stale");
    }

    if let Some(expected_binary) = &options.expected_binary {
        let expected_binary = fs::canonicalize(expected_binary)
            .with_context(|| format!("failed to resolve {}", expected_binary.display()))?;
        let expected_sha = sha256_file(&expected_binary)?;
        status_line("expected_binary", &expected_binary.display().to_string());
        status_line("expected_version", &version_of(&expected_binary));
        status_line("expected_sha", &expected_sha);
        if running_sha != expected_sha {
            println!("STALE: running process does not match expected binary");
            println!(
                "fix: install {} to {} and restart {}",
                expected_binary.display(),
                unit_exec.display(),
                options.unit
            );
            bail!("running systemd process does not match expected binary");
        }
    }

    println!("CURRENT: running systemd service matches installed binary");
    Ok(())
}

fn check_runtime_docker(options: &RuntimeOptions) -> Result<()> {
    status_line("mode", "docker");
    status_line("compose_dir", &options.compose_dir.display().to_string());
    status_line("service", &options.service);

    let mut cid = String::new();
    if options.compose_dir.is_dir() {
        cid = command_output(
            Command::new("docker")
                .args(["compose", "ps", "-q", &options.service])
                .current_dir(&options.compose_dir)
                .stderr(Stdio::null()),
        )
        .unwrap_or_default()
        .trim()
        .to_owned();
    }
    if cid.is_empty() {
        cid = command_output(
            Command::new("docker")
                .args([
                    "ps",
                    "--filter",
                    &format!("name=^/{}$", options.service),
                    "--format",
                    "{{.ID}}",
                ])
                .stderr(Stdio::null()),
        )
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    }
    if cid.is_empty() {
        bail!("FAIL: {} container is not running", options.service);
    }

    let mut image = compose_image(&options.compose_dir).unwrap_or_default();
    if image.is_empty() {
        image = command_output(Command::new("docker").args([
            "inspect",
            &cid,
            "--format",
            "{{.Config.Image}}",
        ]))?
        .trim()
        .to_owned();
    }

    if options.pull && options.compose_dir.is_dir() {
        let status = Command::new("docker")
            .args(["compose", "pull", "--quiet", &options.service])
            .current_dir(&options.compose_dir)
            .status()
            .context("failed to run docker compose pull")?;
        if !status.success() {
            bail!("docker compose pull failed with status {status}");
        }
    }

    let running_image =
        command_output(Command::new("docker").args(["inspect", &cid, "--format", "{{.Image}}"]))?
            .trim()
            .to_owned();
    let local_image = command_output(
        Command::new("docker")
            .args(["image", "inspect", &image, "--format", "{{.Id}}"])
            .stderr(Stdio::null()),
    )
    .unwrap_or_default()
    .trim()
    .to_owned();
    let repo_digests = command_output(
        Command::new("docker")
            .args([
                "image",
                "inspect",
                &image,
                "--format",
                "{{join .RepoDigests \", \"}}",
            ])
            .stderr(Stdio::null()),
    )
    .unwrap_or_default()
    .trim()
    .to_owned();

    status_line("container", &cid);
    status_line("image", &image);
    status_line("running_image_id", &running_image);
    status_line(
        "local_image_id",
        if local_image.is_empty() {
            "missing"
        } else {
            &local_image
        },
    );
    if !repo_digests.is_empty() {
        status_line("repo_digests", &repo_digests);
    }
    if local_image.is_empty() {
        println!("FAIL: compose image is not present locally");
        println!(
            "fix: cd {} && docker compose pull {}",
            options.compose_dir.display(),
            options.service
        );
        bail!("compose image is not present locally");
    }
    if running_image != local_image {
        println!("STALE: running container image differs from local compose image");
        println!(
            "fix: cd {} && docker compose up -d --force-recreate --no-build {}",
            options.compose_dir.display(),
            options.service
        );
        bail!("running container image is stale");
    }

    println!("CURRENT: running container matches local compose image");
    Ok(())
}

fn compose_image(compose_dir: &Path) -> Result<String> {
    if !compose_dir.is_dir() {
        return Ok(String::new());
    }
    let output = command_output(
        Command::new("docker")
            .args(["compose", "config", "--images"])
            .current_dir(compose_dir)
            .stderr(Stdio::null()),
    )?;
    Ok(output.lines().next().unwrap_or_default().to_owned())
}

fn parse_systemd_exec_path(value: &str) -> Option<PathBuf> {
    let (_, after_path) = value.split_once("path=")?;
    let path = after_path.split([' ', ';']).next().unwrap_or_default();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn default_plugin_servers() -> Result<Vec<PluginServer>> {
    let root = std::env::current_dir().context("failed to read current directory")?;
    let workspace = root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.clone());
    Ok(vec![
        PluginServer {
            name: "cortex".to_owned(),
            repo: workspace.join("cortex"),
            binary: "cortex".to_owned(),
            hook: Some("scripts/plugin-setup.sh".into()),
            plugin_root: Some(".".into()),
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: strings(["setup", "plugin-hook", "--no-repair", "--json"]),
            env: pairs([("SYSLOG_MCP_TOKEN", "test-token")]),
            appdata_env: "CLAUDE_PLUGIN_DATA".to_owned(),
            make_appdata: true,
        },
        PluginServer {
            name: "gotify".to_owned(),
            repo: workspace.join("gotify-rmcp"),
            binary: "rgotify".to_owned(),
            hook: Some("plugins/gotify/scripts/plugin-setup.sh".into()),
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: strings(["--json", "setup", "plugin-hook", "--no-repair"]),
            env: Vec::new(),
            appdata_env: "GOTIFY_MCP_HOME".to_owned(),
            make_appdata: true,
        },
        PluginServer {
            name: "unifi".to_owned(),
            repo: workspace.join("unifi-rmcp"),
            binary: "runifi".to_owned(),
            hook: Some("plugins/unifi/scripts/plugin-setup.sh".into()),
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: strings(["--json", "setup", "plugin-hook", "--no-repair"]),
            env: Vec::new(),
            appdata_env: "UNIFI_MCP_HOME".to_owned(),
            make_appdata: true,
        },
        PluginServer {
            name: "tailscale".to_owned(),
            repo: workspace.join("tailscale-rmcp"),
            binary: "rtailscale".to_owned(),
            hook: Some("plugins/tailscale/scripts/plugin-setup.sh".into()),
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: strings(["--json", "setup", "plugin-hook", "--no-repair"]),
            env: Vec::new(),
            appdata_env: "TAILSCALE_MCP_HOME".to_owned(),
            make_appdata: true,
        },
        PluginServer {
            name: "apprise".to_owned(),
            repo: workspace.join("apprise-rmcp"),
            binary: "rapprise".to_owned(),
            hook: Some("plugins/apprise/scripts/plugin-setup.sh".into()),
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: strings(["setup", "plugin-hook", "--no-repair"]),
            env: pairs([
                ("APPRISE_URL", "http://apprise.example:8000"),
                ("APPRISE_MCP_TOKEN", "test-token"),
            ]),
            appdata_env: "CLAUDE_PLUGIN_DATA".to_owned(),
            make_appdata: true,
        },
        PluginServer {
            name: "unraid".to_owned(),
            repo: workspace.join("unraid-rmcp"),
            binary: "runraid".to_owned(),
            hook: Some("plugins/unraid/scripts/plugin-setup.sh".into()),
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: strings(["setup", "plugin-hook", "--no-repair"]),
            env: pairs([
                ("UNRAID_API_URL", "https://tower.example/graphql"),
                ("UNRAID_API_KEY", "test-key"),
                ("UNRAID_MCP_TOKEN", "test-token"),
            ]),
            appdata_env: "UNRAID_HOME".to_owned(),
            make_appdata: true,
        },
        PluginServer {
            name: "soma".to_owned(),
            repo: root.clone(),
            binary: "soma".to_owned(),
            hook: None,
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: strings(["setup", "plugin-hook", "--no-repair"]),
            env: pairs([
                ("SOMA_API_URL", "https://api.example.test"),
                ("SOMA_API_KEY", "test-key"),
                ("SOMA_MCP_TOKEN", "test-token"),
            ]),
            appdata_env: "SOMA_HOME".to_owned(),
            make_appdata: true,
        },
        PluginServer {
            name: "labby".to_owned(),
            repo: workspace.join("lab"),
            binary: "labby".to_owned(),
            hook: None,
            plugin_root: None,
            check_plugin_layout: false,
            package_args: strings(["-p", "labby", "--all-features"]),
            setup_args: strings(["setup", "plugin-hook", "--no-repair", "--json"]),
            env: Vec::new(),
            appdata_env: "LAB_HOME".to_owned(),
            make_appdata: true,
        },
    ])
}

fn check_hook_delegation(server: &PluginServer) -> Result<()> {
    let Some(hook_rel) = &server.hook else {
        return Ok(());
    };
    let hook = server.repo.join(hook_rel);
    if !hook.is_file() {
        bail!("{}: missing hook {}", server.name, hook.display());
    }
    let text = fs::read_to_string(&hook)
        .with_context(|| format!("failed to read hook {}", hook.display()))?;
    validate_hook_text(server, &text)?;
    let status = Command::new("bash")
        .arg("-n")
        .arg(&hook)
        .status()
        .with_context(|| format!("failed to run bash -n {}", hook.display()))?;
    if !status.success() {
        bail!("{}: bash -n failed for {}", server.name, hook.display());
    }
    Ok(())
}

fn validate_hook_text(server: &PluginServer, text: &str) -> Result<()> {
    let expected = format!("{} setup plugin-hook \"$@\"", server.binary);
    let delegates_via_resolved_binary =
        text.contains("}\" setup plugin-hook \"$@\"") && text.contains("command -v");
    if !text.contains(&expected) && !delegates_via_resolved_binary {
        bail!(
            "{}: hook must delegate with `{expected}` or a command-v-resolved binary",
            server.name
        );
    }
    let mut found = Vec::new();
    for token in [
        "cargo build",
        "cargo install",
        "cargo run",
        "docker compose",
        "systemctl",
    ] {
        if text.contains(token) {
            found.push(token);
        }
    }
    if text.contains("curl") && text.contains("| sh") {
        found.push("curl | sh");
    }
    if !found.is_empty() {
        bail!(
            "{}: hook contains forbidden bootstrap tokens: {}",
            server.name,
            found.join(", ")
        );
    }
    Ok(())
}

fn check_plugin_layout(server: &PluginServer) -> Result<()> {
    if !server.check_plugin_layout {
        return Ok(());
    }
    let plugin_root = server.plugin_root.as_ref().map_or_else(
        || server.repo.join(format!("plugins/{}", server.name)),
        |root| server.repo.join(root),
    );
    if !plugin_root.is_dir() {
        bail!(
            "{}: missing plugin root {}",
            server.name,
            plugin_root.display()
        );
    }

    let mut found_manifest = false;
    for relative in [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"] {
        let manifest = plugin_root.join(relative);
        if !manifest.is_file() {
            continue;
        }
        found_manifest = true;
        let payload = read_json_file(&manifest)?;
        if payload.get("version").is_some() {
            bail!(
                "{}: plugin manifest must not contain version: {}",
                server.name,
                manifest.display()
            );
        }
    }
    if !found_manifest {
        bail!(
            "{}: missing plugin manifests under {}",
            server.name,
            plugin_root.display()
        );
    }

    let mut required = Vec::new();
    if plugin_root.join(".mcp.json").exists() {
        required.push(plugin_root.join(".mcp.json"));
    }
    if server.hook.is_some() && plugin_root.join("hooks/hooks.json").exists() {
        required.push(plugin_root.join("hooks/hooks.json"));
    }
    for path in required {
        let _ = read_json_file(&path)?;
    }
    Ok(())
}

fn read_json_file(path: &Path) -> Result<Value> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn check_required_recipes(server: &PluginServer) -> Result<()> {
    let justfile = server.repo.join("Justfile");
    if !justfile.is_file() {
        bail!("{}: missing Justfile", server.name);
    }
    let output = Command::new("just")
        .arg("--list")
        .current_dir(&server.repo)
        .output()
        .with_context(|| format!("failed to run just --list in {}", server.repo.display()))?;
    if !output.status.success() {
        bail!(
            "{}: just --list failed: {}",
            server.name,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let recipes = String::from_utf8_lossy(&output.stdout);
    let missing: Vec<&str> = ["validate-plugin", "runtime-current"]
        .into_iter()
        .filter(|recipe| {
            !recipes.contains(&format!("    {recipe}")) && !recipes.contains(&format!("{recipe}\n"))
        })
        .collect();
    if !missing.is_empty() {
        bail!(
            "{}: missing Justfile recipes: {}",
            server.name,
            missing.join(", ")
        );
    }
    Ok(())
}

fn check_plugin_binary_contract(server: &PluginServer) -> Result<()> {
    let temp = tempfile_dir(format!("{}-plugin-contract-", server.name))?;
    let appdata = temp.join("appdata");
    let log_dir = temp.join("logs");
    if server.make_appdata {
        fs::create_dir(&appdata)?;
    }
    fs::create_dir(&log_dir)?;

    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("--locked")
        .arg("--quiet")
        .args(&server.package_args)
        .arg("--")
        .args(&server.setup_args)
        .current_dir(&server.repo)
        .env(
            "PATH",
            format!(
                "{}:{}",
                server.repo.join("target/debug").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("RUST_LOG", "warn")
        .env("LAB_LOG_DIR", &log_dir)
        .env(&server.appdata_env, &appdata)
        .env("CLAUDE_PLUGIN_DATA", &appdata);
    for (key, value) in &server.env {
        command.env(key, value);
    }

    let output = command
        .output()
        .with_context(|| format!("failed to run setup command for {}", server.name))?;
    fs::remove_dir_all(&temp).ok();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    validate_plugin_setup_stdout(server, &stdout, output.status.success(), &output.stderr)
}

fn validate_plugin_setup_stdout(
    server: &PluginServer,
    stdout: &str,
    success: bool,
    stderr: &[u8],
) -> Result<()> {
    if !stdout.starts_with('{') {
        bail!(
            "{}: setup command did not emit clean JSON on stdout: {:?}; stderr: {:?}",
            server.name,
            stdout.chars().take(120).collect::<String>(),
            String::from_utf8_lossy(stderr)
                .chars()
                .take(240)
                .collect::<String>()
        );
    }
    let payload: Value = serde_json::from_str(stdout)
        .with_context(|| format!("{}: setup stdout is not JSON", server.name))?;
    let missing: BTreeSet<&str> = REQUIRED_PLUGIN_FIELDS
        .into_iter()
        .filter(|field| payload.get(*field).is_none())
        .collect();
    if !missing.is_empty() {
        bail!(
            "{}: JSON missing fields: {}",
            server.name,
            missing.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    if !payload
        .get("exit_policy")
        .and_then(Value::as_str)
        .is_some_and(|policy| EXIT_POLICIES.contains(&policy))
    {
        bail!(
            "{}: invalid exit_policy {:?}",
            server.name,
            payload.get("exit_policy")
        );
    }
    if !payload
        .get("blocking_failures")
        .is_some_and(Value::is_array)
    {
        bail!("{}: blocking_failures must be an array", server.name);
    }
    if !payload
        .get("advisory_failures")
        .is_some_and(Value::is_array)
    {
        bail!("{}: advisory_failures must be an array", server.name);
    }
    if !success && payload.get("exit_policy").and_then(Value::as_str) != Some("blocking_failure") {
        bail!("{}: nonzero exit with non-blocking policy", server.name);
    }
    Ok(())
}

fn default_crawls() -> Vec<CrawlTarget> {
    vec![
        CrawlTarget {
            label: "mcp",
            url: "https://modelcontextprotocol.io",
            domain: "modelcontextprotocol.io",
            target_rel: "mcp/docs",
        },
        CrawlTarget {
            label: "claude-code",
            url: "https://code.claude.com/",
            domain: "code.claude.com",
            target_rel: "claude-code",
        },
    ]
}

fn default_repomix_packs() -> Vec<RepoPack> {
    vec![
        RepoPack {
            remote: "modelcontextprotocol/rust-sdk",
            target_rel: "mcp/repos/modelcontextprotocol-rust-sdk.xml",
            include: "",
            ignore: "",
        },
        RepoPack {
            remote: "modelcontextprotocol/modelcontextprotocol",
            target_rel: "mcp/repos/modelcontextprotocol-modelcontextprotocol.xml",
            include: "docs/**,spec/**",
            ignore: "**/*.svg,**/*.excalidraw.svg",
        },
        RepoPack {
            remote: "modelcontextprotocol/registry",
            target_rel: "mcp/repos/modelcontextprotocol-registry.xml",
            include: "",
            ignore: "**/*.svg,**/*.excalidraw.svg",
        },
        RepoPack {
            remote: "openclaw/mcporter",
            target_rel: "mcporter/repos/openclaw-mcporter.xml",
            include: "",
            ignore: "",
        },
    ]
}

fn crawl_docs(
    options: &RefreshDocsOptions,
    ref_dir: &Path,
    axon_output_dir: &Path,
    target: CrawlTarget,
) -> Result<()> {
    println!(
        "[refresh-docs] crawl {} -> docs/references/{}",
        target.url, target.target_rel
    );
    if options.dry_run {
        return Ok(());
    }
    require_command("axon")?;
    let output = command_output(
        Command::new("axon").args(["crawl", target.url, "--wait", "true", "--yes"]),
    )?;
    print!("{output}");
    let source_dir = parse_axon_job_id(&output)
        .map(|job_id| {
            axon_output_dir
                .join("domains")
                .join(target.domain)
                .join(job_id)
        })
        .filter(|path| path.is_dir())
        .or_else(|| newest_domain_run(axon_output_dir, target.domain))
        .ok_or_else(|| anyhow::anyhow!("could not locate Axon output for {}", target.domain))?;
    copy_job_output_to_layout(&source_dir, &ref_dir.join(target.target_rel))
}

fn parse_axon_job_id(output: &str) -> Option<String> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("Job ID:").map(str::trim))
        .next_back()
        .map(str::to_owned)
}

fn newest_domain_run(axon_output_dir: &Path, domain: &str) -> Option<PathBuf> {
    let domain_dir = axon_output_dir.join("domains").join(domain);
    let entries = fs::read_dir(domain_dir).ok()?;
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let metadata = entry.metadata().ok()?;
            if !metadata.is_dir() {
                return None;
            }
            let modified = metadata.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn copy_job_output_to_layout(source_dir: &Path, target_dir: &Path) -> Result<()> {
    if !source_dir.join("manifest.jsonl").is_file() {
        bail!(
            "ERROR: missing Axon manifest: {}",
            source_dir.join("manifest.jsonl").display()
        );
    }
    if !source_dir.join("markdown").is_dir() {
        bail!(
            "ERROR: missing Axon markdown dir: {}",
            source_dir.join("markdown").display()
        );
    }
    let tmp_target = tempfile_dir("example-refresh-docs.")?;
    copy_dir_all(source_dir, &tmp_target)?;
    atomic_replace_dir(&tmp_target, target_dir)?;
    Ok(())
}

fn pack_repo(options: &RefreshDocsOptions, ref_dir: &Path, pack: RepoPack) -> Result<()> {
    println!(
        "[refresh-docs] pack {} -> docs/references/{}",
        pack.remote, pack.target_rel
    );
    if !pack.include.is_empty() {
        println!("[refresh-docs]   include: {}", pack.include);
    }
    if !pack.ignore.is_empty() {
        println!("[refresh-docs]   ignore:  {}", pack.ignore);
    }
    if options.dry_run {
        return Ok(());
    }
    let tmp_dir = tempfile_dir("example-refresh-docs.")?;
    let tmp_file = tmp_dir.join("repomix-output.xml");
    let mut args = vec![
        "--remote".to_owned(),
        pack.remote.to_owned(),
        "--style".to_owned(),
        "xml".to_owned(),
        "--output".to_owned(),
        tmp_file.display().to_string(),
        "--top-files-len".to_owned(),
        "10".to_owned(),
    ];
    if !pack.include.is_empty() {
        args.push("--include".to_owned());
        args.push(pack.include.to_owned());
    }
    if !pack.ignore.is_empty() {
        args.push("--ignore".to_owned());
        args.push(pack.ignore.to_owned());
    }
    run_repomix(&args)?;
    if !tmp_file.is_file() || fs::metadata(&tmp_file)?.len() == 0 {
        bail!("ERROR: Repomix produced no output for {}", pack.remote);
    }
    let target_file = ref_dir.join(pack.target_rel);
    fs::create_dir_all(target_file.parent().unwrap_or(ref_dir))?;
    fs::rename(&tmp_file, &target_file).with_context(|| {
        format!(
            "failed to move {} to {}",
            tmp_file.display(),
            target_file.display()
        )
    })?;
    fs::remove_dir_all(tmp_dir).ok();
    Ok(())
}

fn run_repomix(args: &[String]) -> Result<()> {
    if let Some(bin) = env_path("REPOMIX_BIN") {
        return run_status(Command::new(bin).args(args));
    }
    if command_exists("repomix") {
        return run_status(Command::new("repomix").args(args));
    }
    require_command("npx")?;
    run_status(Command::new("npx").arg("--yes").arg("repomix").args(args))
}

fn sparse_clone_path(
    options: &RefreshDocsOptions,
    ref_dir: &Path,
    sparse: SparseClone,
) -> Result<()> {
    println!(
        "[refresh-docs] sparse clone {}/{} -> docs/references/{}",
        sparse.remote, sparse.sparse_path, sparse.target_rel
    );
    if options.dry_run {
        return Ok(());
    }
    require_command("git")?;
    let tmp_dir = tempfile_dir("example-refresh-docs.")?;
    let clone_dir = tmp_dir.join("repo");
    let tmp_target = tmp_dir.join("output");
    run_status(Command::new("git").args([
        "clone",
        "--filter=blob:none",
        "--sparse",
        "--depth=1",
        sparse.remote,
        clone_dir.to_str().context("non-UTF-8 clone path")?,
    ]))?;
    run_status(Command::new("git").arg("-C").arg(&clone_dir).args([
        "sparse-checkout",
        "set",
        sparse.sparse_path,
    ]))?;
    fs::create_dir_all(&tmp_target)?;
    match sparse.mode {
        SparseCloneMode::Recursive => {
            copy_dir_all(&clone_dir.join(sparse.sparse_path), &tmp_target)?;
        }
        SparseCloneMode::FlatMdx => {
            for entry in fs::read_dir(clone_dir.join(sparse.sparse_path))? {
                let entry = entry?;
                if entry.path().extension() == Some(OsStr::new("mdx")) {
                    fs::copy(entry.path(), tmp_target.join(entry.file_name()))?;
                }
            }
        }
    }
    atomic_replace_dir(&tmp_target, &ref_dir.join(sparse.target_rel))?;
    fs::remove_dir_all(tmp_dir).ok();
    Ok(())
}

fn write_reference_index(ref_dir: &Path) -> Result<()> {
    let mcp_docs = count_files(&ref_dir.join("mcp/docs"));
    let claude_docs = count_files(&ref_dir.join("claude-code"));
    let mcporter_docs = count_files(&ref_dir.join("mcporter/docs"));
    let updated = command_output(Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]))
        .unwrap_or_else(|_| "unknown".to_owned())
        .trim()
        .to_owned();
    fs::write(
        ref_dir.join("INDEX.md"),
        format!(
            "# Reference Index - soma\n\n\
CUSTOMIZE: When you adapt Soma, update this index to reflect your service's\n\
reference material.\n\n\
| Path | Contents | Source |\n\
| --- | --- | --- |\n\
| `mcp/docs/`        | MCP protocol docs (crawled)    | modelcontextprotocol.io |\n\
| `mcp/repos/`       | MCP Rust SDK + spec (repomix)  | modelcontextprotocol/* |\n\
| `claude-code/`     | Claude Code docs (crawled)     | code.claude.com |\n\
| `mcporter/docs/`   | mcporter docs (sparse clone)   | openclaw/mcporter/docs |\n\
| `mcporter/repos/`  | mcporter source (repomix)      | openclaw/mcporter |\n\n\
## Crawled Doc File Counts\n\n\
| Path | Files |\n\
| --- | ---: |\n\
| `mcp/docs/`      | {mcp_docs} |\n\
| `claude-code/`   | {claude_docs} |\n\
| `mcporter/docs/` | {mcporter_docs} |\n\n\
## Key References for MCP Server Development\n\n\
- **rmcp crate**: `mcp/repos/modelcontextprotocol-rust-sdk.xml`\n\
  The primary reference for implementing ServerHandler, tool dispatch, elicitation, resources, prompts.\n\n\
- **MCP spec**: `mcp/repos/modelcontextprotocol-modelcontextprotocol.xml`\n\
  Protocol specification - useful when the SDK doesn't expose something you need.\n\n\
- **server.json schema**: `mcp/repos/modelcontextprotocol-registry.xml`\n\
  JSON schema for MCP registry publishing (`server.json`).\n\n\
- **mcporter**: `mcporter/repos/openclaw-mcporter.xml`\n\
  Integration testing tool used by `apps/soma/tests/mcporter/test-mcp.sh`.\n\n\
_Updated: {updated}_\n"
        ),
    )
    .with_context(|| format!("failed to write {}", ref_dir.join("INDEX.md").display()))?;
    Ok(())
}

fn refresh_scope(options: &RefreshDocsOptions) -> &'static str {
    if options.skip_crawl {
        "repomix-only"
    } else if options.skip_repomix {
        "crawl-only"
    } else {
        "full"
    }
}

fn snapshot_references(ref_dir: &Path) -> Result<Vec<ReferenceSnapshotEntry>> {
    if !ref_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    collect_snapshot_entries(ref_dir, ref_dir, &mut entries)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn collect_snapshot_entries(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<ReferenceSnapshotEntry>,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_snapshot_entries(root, &path, entries)?;
            continue;
        }
        if path.file_name() == Some(OsStr::new("CHANGES.md")) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(ReferenceSnapshotEntry {
            path: relative,
            sha256: sha256_file(&path)?,
        });
    }
    Ok(())
}

fn summarize_reference_changes(
    changes_file: &Path,
    scope: &str,
    before: &[ReferenceSnapshotEntry],
    after: &[ReferenceSnapshotEntry],
) -> Result<()> {
    let (added, modified, removed) = reference_change_counts(before, after);
    println!(
        "[refresh-docs] change summary: {added} added, {modified} modified, {removed} removed"
    );
    ensure_changes_file(changes_file)?;
    let timestamp = command_output(Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]))
        .unwrap_or_else(|_| "unknown".to_owned())
        .trim()
        .to_owned();
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(changes_file)
        .with_context(|| format!("failed to open {}", changes_file.display()))?;
    writeln!(
        file,
        "\n## {timestamp}\n\n- scope: `{scope}`\n- summary: `{added} added, {modified} modified, {removed} removed`"
    )?;
    Ok(())
}

fn ensure_changes_file(changes_file: &Path) -> Result<()> {
    if changes_file.is_file() {
        return Ok(());
    }
    fs::create_dir_all(changes_file.parent().unwrap_or_else(|| Path::new(".")))?;
    let timestamp = command_output(Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]))
        .unwrap_or_else(|_| "unknown".to_owned())
        .trim()
        .to_owned();
    fs::write(
        changes_file,
        format!(
            "---\n\
title: Reference Refresh Change Log - soma\n\
generated_by: cargo xtask refresh-docs\n\
created_at: {timestamp}\n\
---\n\n\
# Reference Refresh Change Log\n\n\
Each entry records file-level changes after a real refresh run.\n"
        ),
    )?;
    Ok(())
}

fn reference_change_counts(
    before: &[ReferenceSnapshotEntry],
    after: &[ReferenceSnapshotEntry],
) -> (usize, usize, usize) {
    let before_paths: BTreeSet<&str> = before.iter().map(|entry| entry.path.as_str()).collect();
    let after_paths: BTreeSet<&str> = after.iter().map(|entry| entry.path.as_str()).collect();
    let added = after_paths.difference(&before_paths).count();
    let removed = before_paths.difference(&after_paths).count();
    let modified = before_paths
        .intersection(&after_paths)
        .filter(|path| {
            let before_sha = before
                .iter()
                .find(|entry| entry.path == **path)
                .map(|entry| entry.sha256.as_str());
            let after_sha = after
                .iter()
                .find(|entry| entry.path == **path)
                .map(|entry| entry.sha256.as_str());
            before_sha != after_sha
        })
        .count();
    (added, modified, removed)
}

fn atomic_replace_dir(src: &Path, dst: &Path) -> Result<()> {
    let parent = dst.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let backup = tempfile_dir_in(parent, format!(".{}.backup.", basename(dst)))?;
    fs::remove_dir(&backup)?;
    if dst.exists() {
        fs::rename(dst, &backup)
            .with_context(|| format!("failed to move {} to {}", dst.display(), backup.display()))?;
    }
    if let Err(error) = fs::rename(src, dst) {
        if backup.exists() {
            let _ = fs::rename(&backup, dst);
        }
        return Err(error)
            .with_context(|| format!("failed to move {} to {}", src.display(), dst.display()));
    }
    if backup.exists() {
        fs::remove_dir_all(&backup).ok();
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let source = entry.path();
        let target = dst.join(entry.file_name());
        if source.is_dir() {
            copy_dir_all(&source, &target)?;
        } else {
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

fn count_files(path: &Path) -> usize {
    if !path.is_dir() {
        return 0;
    }
    let mut count = 0usize;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files(&path);
            } else {
                count += 1;
            }
        }
    }
    count
}

fn status_line(key: &str, value: &str) {
    println!("{key:<18} {value}");
}

fn version_of(path: &Path) -> String {
    if !path.is_file() {
        return String::new();
    }
    command_output(Command::new(path).arg("--version").stderr(Stdio::null()))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn sha256_file(path: &Path) -> Result<String> {
    Ok(command_output(Command::new("sha256sum").arg(path))?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned())
}

fn require_command(command: &str) -> Result<()> {
    if command_exists(command) {
        Ok(())
    } else {
        bail!("ERROR: required command not found: {command}")
    }
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!(
            "command -v {} >/dev/null 2>&1",
            shell_quote(command)
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_status(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_status(command: &mut Command) -> Result<()> {
    let status = command.status().context("failed to spawn command")?;
    if !status.success() {
        bail!("command failed with status {status}");
    }
    Ok(())
}

fn command_output(command: &mut Command) -> Result<String> {
    let output = command.output().context("failed to spawn command")?;
    if !output.status.success() {
        bail!("command failed with status {}", output.status);
    }
    String::from_utf8(output.stdout).context("command emitted non-UTF-8 stdout")
}

fn git_output<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    command_output(Command::new("git").args(args))
}

fn git_status<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    command_status(Command::new("git").args(args).stderr(Stdio::null()))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn tempfile_dir(prefix: impl AsRef<str>) -> Result<PathBuf> {
    tempfile_dir_in(std::env::temp_dir(), prefix)
}

fn tempfile_dir_in(parent: impl AsRef<Path>, prefix: impl AsRef<str>) -> Result<PathBuf> {
    let parent = parent.as_ref();
    fs::create_dir_all(parent)?;
    for attempt in 0..1000u32 {
        let candidate = parent.join(format!(
            "{}{}-{}",
            prefix.as_ref(),
            std::process::id(),
            attempt
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to create {}", candidate.display()))
            }
        }
    }
    bail!(
        "failed to create temporary directory in {}",
        parent.display()
    )
}

fn basename(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("target")
        .to_owned()
}

fn strings<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_owned).collect()
}

fn pairs<const N: usize>(values: [(&str, &str); N]) -> Vec<(String, String)> {
    values
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_ignores_comments_and_blank_lines() {
        let patterns = parse_allowlist(
            r#"
# comment
docs/reference/**
assets/*.png # real assets

"#,
        );
        assert_eq!(patterns, vec!["docs/reference/**", "assets/*.png"]);
        assert!(is_allowlisted("assets/logo.png", &patterns));
        assert!(is_allowlisted("docs/reference/mcp/index.md", &patterns));
        assert!(!is_allowlisted("src/main.rs", &patterns));
    }

    #[test]
    fn blob_options_match_python_defaults_and_overrides() {
        let args = strings([
            "--base",
            "origin/dev",
            "--head",
            "feature",
            "--max-bytes",
            "42",
            "--allowlist",
            "allow.txt",
        ]);
        let parsed = BlobSizeOptions::parse(&args).unwrap();
        assert_eq!(parsed.base, "origin/dev");
        assert_eq!(parsed.head, "feature");
        assert_eq!(parsed.max_bytes, 42);
        assert_eq!(parsed.allowlist, PathBuf::from("allow.txt"));
    }

    #[test]
    fn blob_summary_matches_script_shape() {
        let blob = ChangedBlob {
            path: "big.bin".to_owned(),
            size_bytes: 1024,
            is_allowlisted: false,
            is_binary: true,
        };
        let violations = vec![&blob];
        let temp = tempfile_dir("lane-c-summary.").unwrap();
        let summary = temp.join("summary.md");
        std::env::set_var("GITHUB_STEP_SUMMARY", &summary);
        write_blob_step_summary(512, std::slice::from_ref(&blob), &violations).unwrap();
        std::env::remove_var("GITHUB_STEP_SUMMARY");
        let text = fs::read_to_string(summary).unwrap();
        assert!(text.contains("## Blob Size Policy"));
        assert!(text.contains("| `big.bin` | binary | `1024` bytes (1.0 KiB) | blocked |"));
        fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn dependency_parser_matches_awks_section_scope() {
        let deps = extract_direct_deps_from_toml(
            r#"
[package]
name = "x"

[workspace.dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
local = { path = "crates/local" }

[profile.release]
debug = false
"#,
        );
        assert_eq!(
            deps,
            vec![
                DirectDependency {
                    name: "anyhow".to_owned(),
                    line: "anyhow = \"1\"".to_owned()
                },
                DirectDependency {
                    name: "local".to_owned(),
                    line: "local = { path = \"crates/local\" }".to_owned()
                },
                DirectDependency {
                    name: "serde".to_owned(),
                    line: "serde = { version = \"1\", features = [\"derive\"] }".to_owned()
                },
            ]
        );
        assert_eq!(dependency_version_req(&deps[0].line).as_deref(), Some("1"));
        assert_eq!(dependency_version_req(&deps[1].line), None);
        assert_eq!(dependency_version_req(&deps[2].line).as_deref(), Some("1"));
    }

    #[test]
    fn dependency_status_preserves_shell_cases() {
        assert_eq!(direct_dependency_status("1.2.3", "1.2.3"), "ok");
        assert_eq!(direct_dependency_status("1", "1.9.0"), "compatible-range");
        assert_eq!(direct_dependency_status("1.2", "1.2.9"), "compatible-range");
        assert_eq!(direct_dependency_status("^1", "2.0.0"), "review");
    }

    #[test]
    fn dry_run_update_detector_matches_cargo_output_patterns() {
        assert!(dry_run_reports_updates(
            "    Updating anyhow v1.0.0 -> v1.0.1\n"
        ));
        assert!(dry_run_reports_updates(
            "    Locking 2 packages to latest compatible versions\n"
        ));
        assert!(!dry_run_reports_updates(
            "    Locking 0 packages to latest compatible versions\n"
        ));
    }

    #[test]
    fn runtime_args_and_execstart_parser_match_shell_script() {
        let mut options = RuntimeOptions::from_env();
        options
            .parse_args(&strings([
                "--mode",
                "docker",
                "--pull",
                "--unit",
                "x.service",
                "--service",
                "svc",
                "--compose-dir",
                "/tmp/compose",
                "--expected-binary",
                "/bin/echo",
            ]))
            .unwrap();
        assert_eq!(options.mode, RuntimeMode::Docker);
        assert!(options.pull);
        assert_eq!(options.unit, "x.service");
        assert_eq!(options.service, "svc");
        assert_eq!(options.compose_dir, PathBuf::from("/tmp/compose"));
        assert_eq!(options.expected_binary, Some(PathBuf::from("/bin/echo")));

        assert_eq!(
            parse_systemd_exec_path("{ path=/home/me/bin/soma ; argv[]=/home/me/bin/soma serve; }"),
            Some(PathBuf::from("/home/me/bin/soma"))
        );
    }

    #[test]
    fn plugin_hook_validation_blocks_bootstrap_work() {
        let server = PluginServer {
            name: "demo".to_owned(),
            repo: PathBuf::new(),
            binary: "demo".to_owned(),
            hook: None,
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: Vec::new(),
            env: Vec::new(),
            appdata_env: "DEMO_HOME".to_owned(),
            make_appdata: true,
        };
        validate_hook_text(&server, "exec demo setup plugin-hook \"$@\"").unwrap();
        let error =
            validate_hook_text(&server, "cargo run -- demo setup plugin-hook \"$@\"").unwrap_err();
        assert!(error.to_string().contains("forbidden bootstrap tokens"));
    }

    #[test]
    fn plugin_setup_json_contract_checks_required_fields() {
        let server = PluginServer {
            name: "demo".to_owned(),
            repo: PathBuf::new(),
            binary: "demo".to_owned(),
            hook: None,
            plugin_root: None,
            check_plugin_layout: true,
            package_args: Vec::new(),
            setup_args: Vec::new(),
            env: Vec::new(),
            appdata_env: "DEMO_HOME".to_owned(),
            make_appdata: true,
        };
        validate_plugin_setup_stdout(
            &server,
            r#"{"exit_policy":"success","ran_repair":false,"no_repair":true,"blocking_failures":[],"advisory_failures":[]}"#,
            true,
            b"",
        )
        .unwrap();
        let error = validate_plugin_setup_stdout(
            &server,
            r#"{"exit_policy":"success","blocking_failures":[],"advisory_failures":[]}"#,
            true,
            b"",
        )
        .unwrap_err();
        assert!(error.to_string().contains("JSON missing fields"));
    }

    #[test]
    fn refresh_docs_options_and_scope_match_shell() {
        let options = RefreshDocsOptions::parse(&strings(["--dry-run", "--skip-crawl"])).unwrap();
        assert!(options.dry_run);
        assert!(options.skip_crawl);
        assert_eq!(refresh_scope(&options), "repomix-only");
        let options = RefreshDocsOptions::parse(&strings(["--skip-repomix"])).unwrap();
        assert_eq!(refresh_scope(&options), "crawl-only");
        let options = RefreshDocsOptions::parse(&[]).unwrap();
        assert_eq!(refresh_scope(&options), "full");
    }

    #[test]
    fn axon_job_id_parser_uses_last_job_line() {
        assert_eq!(
            parse_axon_job_id("noise\nJob ID: first\nJob ID: second\n").as_deref(),
            Some("second")
        );
    }

    #[test]
    fn reference_change_summary_counts_added_modified_removed() {
        let before = vec![
            ReferenceSnapshotEntry {
                path: "a.md".to_owned(),
                sha256: "1".to_owned(),
            },
            ReferenceSnapshotEntry {
                path: "b.md".to_owned(),
                sha256: "2".to_owned(),
            },
        ];
        let after = vec![
            ReferenceSnapshotEntry {
                path: "b.md".to_owned(),
                sha256: "3".to_owned(),
            },
            ReferenceSnapshotEntry {
                path: "c.md".to_owned(),
                sha256: "4".to_owned(),
            },
        ];
        assert_eq!(reference_change_counts(&before, &after), (1, 1, 1));
    }

    #[test]
    fn cargo_search_version_parser_matches_script_awk() {
        assert_eq!(
            parse_cargo_search_version("anyhow", "anyhow = \"1.0.99\" # error handling\n"),
            Some("1.0.99".to_owned())
        );
        assert_eq!(
            parse_cargo_search_version("anyhow", "not-anyhow = \"9\"\n"),
            None
        );
    }

    #[test]
    fn flat_mdx_variant_is_kept_for_future_service_docs() {
        assert!(matches!(SparseCloneMode::FlatMdx, SparseCloneMode::FlatMdx));
    }
}
