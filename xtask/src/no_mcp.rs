use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

const NO_MCP_REF: &str = "marketplace-no-mcp";

pub fn apply_cmd() -> Result<()> {
    let root = std::env::current_dir().context("failed to read current directory")?;
    let changed = apply(&root)?;
    if changed.is_empty() {
        println!("no-MCP marketplace rules already applied");
    } else {
        for change in changed {
            println!("{change}");
        }
    }
    Ok(())
}

pub fn check_cmd(args: &[String]) -> Result<()> {
    let mut check_current = false;
    let mut compare_ref = false;
    let mut skip_if_missing_refs = false;
    let mut main_ref = "origin/main".to_owned();
    let mut no_mcp_ref = format!("origin/{NO_MCP_REF}");

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--check-current" => check_current = true,
            "--compare-ref" => compare_ref = true,
            "--skip-if-missing-refs" => skip_if_missing_refs = true,
            "--main-ref" => {
                index += 1;
                main_ref = args
                    .get(index)
                    .context("--main-ref requires a value")?
                    .to_owned();
            }
            "--no-mcp-ref" => {
                index += 1;
                no_mcp_ref = args
                    .get(index)
                    .context("--no-mcp-ref requires a value")?
                    .to_owned();
            }
            "--help" | "-h" => {
                bail!("Usage: cargo xtask check-no-mcp-drift [--check-current|--compare-ref] [--skip-if-missing-refs] [--main-ref REF] [--no-mcp-ref REF]");
            }
            unknown => bail!("unknown check-no-mcp-drift option: {unknown}"),
        }
        index += 1;
    }

    let root = std::env::current_dir().context("failed to read current directory")?;
    if compare_ref {
        return compare_refs(&root, &main_ref, &no_mcp_ref, skip_if_missing_refs);
    }
    if check_current || current_branch(&root)? == NO_MCP_REF {
        return check_current_invariants(&root);
    }
    println!("not on marketplace-no-mcp; no-MCP current-branch check skipped");
    Ok(())
}

fn apply(root: &Path) -> Result<Vec<String>> {
    let mut changed = Vec::new();

    for path in mcp_config_files(root) {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove {}", display_path(root, &path)))?;
        changed.push(format!("removed MCP config: {}", display_path(root, &path)));
    }

    for path in manifest_files(root) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut data) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(object) = data.as_object_mut() else {
            continue;
        };
        if object.remove("mcpServers").is_some() {
            write_json(&path, &data)
                .with_context(|| format!("failed to write {}", display_path(root, &path)))?;
            changed.push(format!(
                "stripped manifest MCP servers: {}",
                display_path(root, &path)
            ));
        }
    }

    Ok(changed)
}

fn check_current_invariants(root: &Path) -> Result<()> {
    let mut errors = Vec::new();

    for path in mcp_config_files(root) {
        errors.push(format!(
            "no-MCP branch must not contain {}",
            display_path(root, &path)
        ));
    }

    for path in manifest_files(root) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(data) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if data
            .as_object()
            .is_some_and(|object| object.contains_key("mcpServers"))
        {
            errors.push(format!(
                "no-MCP branch manifest must not declare mcpServers: {}",
                display_path(root, &path)
            ));
        }
    }

    if errors.is_empty() {
        println!("no-MCP invariant check passed");
        Ok(())
    } else {
        eprintln!("no-MCP invariant check failed:");
        for error in &errors {
            eprintln!("- {error}");
        }
        bail!("no-MCP invariant check failed")
    }
}

fn compare_refs(
    root: &Path,
    main_ref: &str,
    no_mcp_ref: &str,
    skip_if_missing_refs: bool,
) -> Result<()> {
    let fetch = run_git(
        root,
        &[
            "fetch",
            "origin",
            &fetch_ref_name(main_ref),
            &fetch_ref_name(no_mcp_ref),
        ],
    );
    if let Err(error) = fetch {
        if skip_if_missing_refs {
            eprintln!("no-MCP drift compare skipped because refs could not be fetched:\n{error:#}");
            return Ok(());
        }
        return Err(error);
    }

    let temp_parent = std::env::temp_dir().join(format!(
        "rtemplate-no-mcp-drift.{}.{}",
        std::process::id(),
        nanos_since_epoch()
    ));
    let expected = temp_parent.join("expected");
    let actual = temp_parent.join("actual");
    fs::create_dir_all(&temp_parent)
        .with_context(|| format!("failed to create {}", temp_parent.display()))?;

    let result = (|| {
        run_git(
            root,
            &[
                "worktree",
                "add",
                "--detach",
                path_str(&expected)?,
                main_ref,
            ],
        )?;
        run_git(
            root,
            &[
                "worktree",
                "add",
                "--detach",
                path_str(&actual)?,
                no_mcp_ref,
            ],
        )?;

        apply(&expected)?;
        check_current_invariants(&expected)?;
        check_current_invariants(&actual)?;

        run_git(&expected, &["add", "-A"])?;
        let expected_tree = run_git_output(&expected, &["write-tree"])?;
        let actual_tree = run_git_output(&actual, &["rev-parse", "HEAD^{tree}"])?;
        if expected_tree.trim() != actual_tree.trim() {
            let diff = run_git_output(
                &expected,
                &["diff", "--stat", actual_tree.trim(), expected_tree.trim()],
            )
            .unwrap_or_else(|error| format!("failed to render diff stat: {error:#}"));
            eprintln!("{no_mcp_ref} does not match {main_ref} plus no-MCP transform:");
            eprintln!("{diff}");
            bail!("no-MCP drift compare failed");
        }

        println!("no-MCP drift compare passed");
        println!("- main ref: {main_ref}");
        println!("- no-MCP ref: {no_mcp_ref}");
        println!("- tree: {}", actual_tree.trim());
        Ok(())
    })();

    for path in [&expected, &actual] {
        if path.exists() {
            let _ = run_git(
                root,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    path_str(path).unwrap_or(""),
                ],
            );
        }
    }
    let _ = fs::remove_dir_all(&temp_parent);
    result
}

fn mcp_config_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !is_ignored(entry.path()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy();
            if name == ".mcp.json" || name == "mcp.json" {
                Some(entry.path().to_path_buf())
            } else {
                None
            }
        })
        .collect()
}

fn manifest_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !is_ignored(entry.path()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy();
            let parent = path.parent()?.file_name()?.to_string_lossy();
            if name == "gemini-extension.json"
                || (name == "plugin.json"
                    && (parent == ".claude-plugin" || parent == ".codex-plugin"))
            {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
        .collect()
}

fn is_ignored(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".git" | "target" | "node_modules" | ".next" | "dist" | ".cache"
            )
        })
}

fn write_json(path: &Path, data: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(data)?;
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

fn current_branch(root: &Path) -> Result<String> {
    Ok(run_git_output(root, &["branch", "--show-current"])?
        .trim()
        .to_owned())
}

fn fetch_ref_name(ref_name: &str) -> String {
    ref_name
        .strip_prefix("origin/")
        .unwrap_or(ref_name)
        .to_owned()
}

fn run_git(root: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error("git", args, output))
    }
}

fn run_git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(command_error("git", args, output))
    }
}

fn command_error(command: &str, args: &[&str], output: std::process::Output) -> anyhow::Error {
    anyhow!(
        "`{} {}` failed with exit {}:\n{}{}",
        command,
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn nanos_since_epoch() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}
