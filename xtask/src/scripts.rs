//! Rust implementations for small scripts that still have compatibility wrappers
//! in `scripts/`.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{run_cargo, run_cmd_output};

pub fn block_env_commits() -> Result<()> {
    let staged = git_output(&["diff", "--cached", "--name-only"])?;
    let blocked: Vec<&str> = staged
        .lines()
        .filter(|path| is_blocked_env_path(path))
        .collect();

    if blocked.is_empty() {
        return Ok(());
    }

    eprintln!("block-env-commits: BLOCKED - .env file(s) staged for commit:");
    for path in &blocked {
        eprintln!("  {path}");
    }
    eprintln!();
    eprintln!("Only .env.example is allowed to be committed.");
    eprintln!("Remove the staged file(s) with: git restore --staged <file>");
    eprintln!("Then add them to .gitignore if they aren't already.");
    bail!(".env file(s) staged for commit")
}

pub fn check_coupled_files(args: &[String]) -> Result<()> {
    if matches!(args.first().map(String::as_str), Some("--help" | "-h")) {
        println!("Usage: cargo xtask check-coupled-files [BASE] [HEAD]");
        return Ok(());
    }
    if args.len() > 2 {
        bail!("Usage: cargo xtask check-coupled-files [BASE] [HEAD]");
    }

    let mut base = args.first().map(String::as_str).unwrap_or("origin/main");
    let head = args.get(1).map(String::as_str).unwrap_or("HEAD");
    if !git_ref_exists(base) {
        base = "HEAD~1";
    }

    let changed = git_output(&["diff", "--name-only", base, head])?;
    let changed: Vec<&str> = changed.lines().collect();
    let mut issues = Vec::new();

    if changed_path(&changed, "Justfile") && !changed_path(&changed, "lefthook.yml") {
        issues.push("Justfile changed but lefthook.yml did not; confirm hook/recipe parity.");
    }
    if changed_path(&changed, "lefthook.yml") && !changed_path(&changed, "Justfile") {
        issues.push(
            "lefthook.yml changed but Justfile did not; confirm matching manual recipe exists.",
        );
    }
    if changed_path(&changed, "scripts/*") && !changed_path(&changed, "scripts/README.md") {
        issues.push("scripts changed but scripts/README.md did not; document new or changed script behavior.");
    }
    if changed_path(&changed, "crates/rtemplate-mcp/src/schemas.rs")
        && !changed_path(&changed, "docs/MCP_SCHEMA.md")
    {
        issues.push("crates/rtemplate-mcp/src/schemas.rs changed but docs/MCP_SCHEMA.md did not; run scripts/check-schema-docs.py --write.");
    }
    if changed_path(&changed, "plugins/rtemplate/*") && !changed_path(&changed, "docs/PLUGINS.md") {
        issues.push("plugin package changed but docs/PLUGINS.md did not; confirm plugin docs are still current.");
    }

    if !issues.is_empty() {
        eprintln!("Coupled-file check failed:");
        for issue in &issues {
            eprintln!("  - {issue}");
        }
        bail!("coupled-file check failed");
    }

    println!("Coupled-file check passed ({base}..{head}).");
    Ok(())
}

pub fn sync_cargo() -> Result<()> {
    let repo_root = env_path("CLAUDE_PLUGIN_ROOT").unwrap_or_else(current_dir);
    let data_root = env_path("CLAUDE_PLUGIN_DATA").unwrap_or_else(|| repo_root.clone());
    let src_lock = repo_root.join("Cargo.lock");
    let dst_lock = data_root.join("Cargo.lock");

    if !src_lock.is_file() {
        bail!("sync-cargo.sh: missing lockfile at {}", src_lock.display());
    }

    if same_file_bytes(&src_lock, &dst_lock)? {
        return Ok(());
    }

    std::fs::create_dir_all(&data_root)
        .with_context(|| format!("failed to create {}", data_root.display()))?;

    if let Err(copy_error) = std::fs::copy(&src_lock, &dst_lock) {
        eprintln!(
            "sync-cargo: failed to copy {} to {}: {copy_error}; falling back to cargo fetch",
            src_lock.display(),
            dst_lock.display()
        );
        if let Err(fetch_error) = run_cargo_fetch(&repo_root) {
            let _ = std::fs::remove_file(&dst_lock);
            return Err(fetch_error);
        }
    }

    Ok(())
}

pub fn run_ascii_check(args: &[String]) -> Result<()> {
    let fix = match args {
        [] => false,
        [arg] if arg == "--fix" => true,
        [arg] if arg == "--help" || arg == "-h" => {
            println!("Usage: cargo xtask run-ascii-check [--fix]");
            return Ok(());
        }
        _ => bail!("Usage: cargo xtask run-ascii-check [--fix]"),
    };

    let output = git_output(&[
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
    ])?;
    let files: Vec<String> = output
        .lines()
        .filter(|path| Path::new(path).is_file())
        .map(str::to_owned)
        .collect();

    if files.is_empty() {
        println!("No files to check");
        return Ok(());
    }

    let mut command = Command::new("python3");
    command.arg("scripts/asciicheck.py");
    if fix {
        command.arg("--fix");
    }
    command.args(&files);
    let status = command
        .stdin(Stdio::null())
        .status()
        .context("failed to spawn python3")?;
    if !status.success() {
        bail!("ascii check failed with status {status}");
    }
    Ok(())
}

pub fn check_plugin_stdio_smoke() -> Result<()> {
    let bin = std::env::var("BIN").unwrap_or_else(|_| "rtemplate".to_owned());
    let timeout_secs = std::env::var("TIMEOUT_SECS").unwrap_or_else(|_| "5".to_owned());

    if !command_on_path(&bin) {
        bail!("plugin stdio smoke: {bin} is not on PATH\nrun: just install-local");
    }

    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"plugin-stdio-smoke","version":"0.0.0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"example","arguments":{"action":"status"}}}"#,
    ]
    .join("\n");

    let mut child = Command::new("timeout")
        .arg(format!("{timeout_secs}s"))
        .arg(&bin)
        .arg("mcp")
        .env("RTEMPLATE_API_URL", "")
        .env("RUST_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn timeout/{bin}"))?;

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().context("failed to open child stdin")?;
        stdin
            .write_all(input.as_bytes())
            .context("failed to write JSON-RPC smoke input")?;
        stdin
            .write_all(b"\n")
            .context("failed to write trailing newline")?;
    }

    let output = child
        .wait_with_output()
        .context("failed to read plugin stdio smoke output")?;
    if !output.status.success() {
        bail!("plugin stdio smoke command exited with {}", output.status);
    }
    let stdout =
        String::from_utf8(output.stdout).context("plugin stdio smoke emitted non-UTF-8 stdout")?;

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("invalid JSON-RPC line from stdio smoke: {line}"))?;
        if value.get("id").and_then(serde_json::Value::as_i64) != Some(2) {
            continue;
        }
        if value
            .pointer("/result/structuredContent/status")
            .and_then(serde_json::Value::as_str)
            == Some("ok")
        {
            println!("plugin stdio smoke passed");
            return Ok(());
        }
        bail!("plugin stdio smoke response for id=2 did not report status=ok: {value}");
    }

    bail!("plugin stdio smoke did not receive a tools/call response with id=2")
}

fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn git_output(args: &[&str]) -> Result<String> {
    run_cmd_output("git", args)
}

fn git_ref_exists(ref_name: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", ref_name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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

fn is_blocked_env_path(path: &str) -> bool {
    if path.ends_with(".env.example") {
        return false;
    }
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.contains(".env"))
}

fn changed_path(paths: &[&str], pattern: &str) -> bool {
    paths.iter().any(|path| glob_match(pattern, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        path.starts_with(prefix) && path.ends_with(suffix)
    } else {
        path == pattern
    }
}

fn same_file_bytes(left: &Path, right: &Path) -> Result<bool> {
    if !right.exists() {
        return Ok(false);
    }
    let left = std::fs::read(left).with_context(|| format!("failed to read {}", left.display()))?;
    let right =
        std::fs::read(right).with_context(|| format!("failed to read {}", right.display()))?;
    Ok(left == right)
}

fn run_cargo_fetch(repo_root: &Path) -> Result<()> {
    let manifest = repo_root.join("Cargo.toml");
    let manifest = manifest
        .to_str()
        .with_context(|| format!("non-UTF-8 manifest path: {}", manifest.display()))?;
    run_cargo(&["fetch", "--manifest-path", manifest])
}

#[cfg(test)]
mod tests {
    use super::{changed_path, command_on_path, glob_match, is_blocked_env_path};

    #[test]
    fn blocks_env_files_except_examples() {
        assert!(is_blocked_env_path(".env"));
        assert!(is_blocked_env_path("config/.env.local"));
        assert!(is_blocked_env_path("services/foo.env.prod"));
        assert!(!is_blocked_env_path(".env.example"));
        assert!(!is_blocked_env_path("docs/env.example.md"));
    }

    #[test]
    fn matches_bash_style_single_star_patterns_used_by_coupled_check() {
        assert!(glob_match("scripts/*", "scripts/check-coupled-files.sh"));
        assert!(glob_match(
            "plugins/rtemplate/*",
            "plugins/rtemplate/hooks/setup.sh"
        ));
        assert!(glob_match("Justfile", "Justfile"));
        assert!(!glob_match("Justfile", "docs/Justfile"));
    }

    #[test]
    fn changed_path_checks_any_changed_path() {
        let paths = ["README.md", "scripts/check-coupled-files.sh"];
        assert!(changed_path(&paths, "scripts/*"));
        assert!(!changed_path(&paths, "lefthook.yml"));
    }

    #[test]
    fn command_on_path_handles_absolute_missing_path() {
        assert!(!command_on_path("/definitely/not/a/real/rtemplate"));
    }
}
