use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const OUTPUT_KEYS: &[&str] = &[
    "all",
    "docs",
    "workflow",
    "rust",
    "web",
    "native",
    "mcp",
    "docker",
    "docker_build",
    "toml",
    "soma",
    "security",
    "secrets",
    "release",
    "python",
    "wasm_component",
];

pub fn run(args: &[String]) -> Result<()> {
    let options = Options::parse(args)?;
    let paths = match &options.changed_files {
        Some(path) => read_paths(path)?,
        None => resolve_paths(&options.event)?,
    };

    if let Some(path) = &options.write_changed_files {
        fs::write(
            path,
            paths.join("\n") + if paths.is_empty() { "" } else { "\n" },
        )
        .with_context(|| format!("write changed files to {}", path.display()))?;
    }

    let values = classify(&options.event, &paths);
    write_outputs(&options.output, &values)?;

    for key in OUTPUT_KEYS {
        println!("{key}={}", values[*key]);
    }

    Ok(())
}

#[derive(Debug)]
struct Options {
    event: String,
    changed_files: Option<PathBuf>,
    output: PathBuf,
    write_changed_files: Option<PathBuf>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut event = None;
        let mut changed_files = None;
        let mut output = None;
        let mut write_changed_files = None;
        let mut index = 0usize;

        while index < args.len() {
            match args[index].as_str() {
                "--event" => {
                    index += 1;
                    event = Some(
                        args.get(index)
                            .context("--event requires a value")?
                            .to_owned(),
                    );
                }
                "--changed-files" => {
                    index += 1;
                    changed_files = Some(PathBuf::from(
                        args.get(index)
                            .context("--changed-files requires a value")?,
                    ));
                }
                "--output" => {
                    index += 1;
                    output = Some(PathBuf::from(
                        args.get(index).context("--output requires a value")?,
                    ));
                }
                "--write-changed-files" => {
                    index += 1;
                    write_changed_files = Some(PathBuf::from(
                        args.get(index)
                            .context("--write-changed-files requires a value")?,
                    ));
                }
                "--help" | "-h" => bail!(
                    "Usage: cargo xtask changed-paths --event <event> --output <path> [--changed-files <path>] [--write-changed-files <path>]"
                ),
                unknown => bail!("unknown changed-paths option: {unknown}"),
            }
            index += 1;
        }

        Ok(Self {
            event: event.context("--event is required")?,
            changed_files,
            output: output.context("--output is required")?,
            write_changed_files,
        })
    }
}

fn classify(event: &str, paths: &[String]) -> BTreeMap<String, bool> {
    if event == "workflow_dispatch" {
        return all_enabled();
    }
    if paths.is_empty() {
        return all_enabled();
    }

    let workflow = any(paths, |p| {
        starts(p, &[".github/"])
            || matches!(
                p,
                "xtask/src/ci_paths.rs" | "xtask/src/main.rs" | "docs/CI.md"
            )
    });
    // The all-true fail-safe promotion below is reserved for changes that can
    // alter CI routing itself: ci.yml, shared composite actions, this
    // classifier, or its documentation. Other workflow files (release,
    // monitors, docs, conformance) still set `workflow` so actionlint and the
    // lefthook policy run, but no longer force every expensive job.
    let workflow_core = any(paths, |p| {
        starts(p, &[".github/actions/"])
            || matches!(
                p,
                ".github/workflows/ci.yml"
                    | "xtask/src/ci_paths.rs"
                    | "xtask/src/main.rs"
                    | "docs/CI.md"
            )
    });
    let docs = any(paths, |p| {
        (starts(p, &["docs/"]) && !starts(p, &["docs/sessions/"]))
            || matches!(
                p,
                "README.md" | "CHANGELOG.md" | "CLAUDE.md" | "AGENTS.md" | "GEMINI.md"
            )
    });
    let web = any(paths, |p| {
        starts(p, &["apps/web/", "crates/soma/web/"])
            || matches!(p, "package.json" | "pnpm-lock.yaml" | "pnpm-workspace.yaml")
    });
    let mcp = any(paths, |p| {
        starts(
            p,
            &[
                "crates/soma/mcp/",
                "crates/soma/api/",
                // crates/soma/contracts was split (plan PR 13) and deleted
                // (PR 19); the pieces that shape MCP tool schemas and server
                // startup env now live in soma-domain (ACTION_SPECS) and
                // soma-config (McpConfig, env prefixes).
                "crates/soma/domain/",
                "crates/soma/config/",
                "apps/soma/tests/mcporter/",
                "docs/reference/mcp/",
            ],
        )
    });
    let rust = any(paths, |p| {
        starts(p, &["apps/soma/", "crates/", "xtask/"])
            || matches!(
                p,
                "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | ".cargo/config.toml"
            )
    });
    let docker = rust
        || web
        || any(paths, |p| {
            starts(p, &["config/", "scripts/"])
                || matches!(
                    p,
                    ".dockerignore"
                        | ".env.example"
                        | "docker-compose.yml"
                        | "docker-compose.prod.yml"
                )
        });
    // Narrower than `docker`: true only when the container image inputs
    // themselves change. Gates the (expensive, hosted) container-smoke job on
    // PRs; pushes to main always smoke regardless of this flag.
    let docker_build = any(paths, |p| {
        matches!(
            p,
            "config/Dockerfile"
                | ".dockerignore"
                | ".env.example"
                | "docker-compose.yml"
                | "docker-compose.prod.yml"
        )
    });
    let toml = any(paths, |p| p.ends_with(".toml"));
    let soma = rust
        || mcp
        || docs
        || any(paths, |p| {
            starts(p, &["plugins/", "scaffold/", "cargo-generate/", ".claude/"])
                || matches!(p, "Justfile" | "lefthook.yml")
        });
    let security = rust
        || any(paths, |p| {
            matches!(p, "Cargo.lock" | "deny.toml") || starts(p, &["vendor/"])
        });
    let native = rust || web;
    let release = rust || web || any(paths, |p| starts(p, &["release/", "packages/soma-rmcp/"]));
    let python = any(paths, |p| starts(p, &["packages/python/"]));
    let wasm_component = any(paths, |p| starts(p, &["examples/providers/components/"]));

    let mut result = BTreeMap::new();
    result.insert("all".to_owned(), false);
    result.insert("docs".to_owned(), docs);
    result.insert("workflow".to_owned(), workflow);
    result.insert("rust".to_owned(), rust);
    result.insert("web".to_owned(), web);
    result.insert("native".to_owned(), native);
    result.insert("mcp".to_owned(), mcp);
    result.insert("docker".to_owned(), docker);
    result.insert("docker_build".to_owned(), docker_build);
    result.insert("toml".to_owned(), toml);
    result.insert("soma".to_owned(), soma);
    result.insert("security".to_owned(), security);
    result.insert(
        "secrets".to_owned(),
        !only_low_risk_docs_or_agent_files(paths),
    );
    result.insert("release".to_owned(), release);
    result.insert("python".to_owned(), python);
    result.insert("wasm_component".to_owned(), wasm_component);

    if workflow_core {
        for key in OUTPUT_KEYS {
            result.insert((*key).to_owned(), true);
        }
    }

    result
}

fn all_enabled() -> BTreeMap<String, bool> {
    OUTPUT_KEYS
        .iter()
        .map(|key| ((*key).to_owned(), true))
        .collect()
}

fn only_low_risk_docs_or_agent_files(paths: &[String]) -> bool {
    paths.iter().all(|p| {
        starts(p, &[".agents/skills/", "docs/sessions/"])
            || (starts(p, &["docs/"]) && p.ends_with(".md"))
    })
}

fn starts(path: &str, prefixes: &[&str]) -> bool {
    prefixes
        .iter()
        .any(|prefix| path == prefix.trim_end_matches('/') || path.starts_with(*prefix))
}

fn any(paths: &[String], predicate: impl Fn(&str) -> bool) -> bool {
    paths.iter().any(|path| predicate(path))
}

fn read_paths(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn resolve_paths(event: &str) -> Result<Vec<String>> {
    if event == "workflow_dispatch" {
        return Ok(Vec::new());
    }

    let head = std::env::var("PR_HEAD_SHA")
        .or_else(|_| std::env::var("HEAD_SHA"))
        .or_else(|_| std::env::var("GITHUB_SHA"))
        .unwrap_or_else(|_| "HEAD".to_owned());
    let mut base = match event {
        "pull_request" => std::env::var("PR_BASE_SHA").unwrap_or_default(),
        "push" => {
            if std::env::var("GITHUB_REF")
                .unwrap_or_default()
                .starts_with("refs/tags/")
            {
                return Ok(Vec::new());
            }
            std::env::var("PUSH_BEFORE_SHA").unwrap_or_default()
        }
        _ => return Ok(Vec::new()),
    };

    if base.is_empty() || base.chars().all(|ch| ch == '0') || !git_ref_exists(&base) {
        base = git_output(&["rev-parse", "HEAD^"]).unwrap_or_default();
    }
    if base.is_empty() {
        return Ok(Vec::new());
    }

    let raw = git_output(&["diff", "--name-only", &base, &head]).unwrap_or_default();
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn git_ref_exists(rev: &str) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", rev])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn git_output(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn write_outputs(path: &Path, values: &BTreeMap<String, bool>) -> Result<()> {
    let content = OUTPUT_KEYS
        .iter()
        .map(|key| format!("{key}={}", values[*key]))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify_paths(paths: &[&str]) -> BTreeMap<String, bool> {
        classify(
            "pull_request",
            &paths
                .iter()
                .map(|path| (*path).to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn agent_skill_changes_skip_expensive_ci() {
        let out = classify_paths(&[".agents/skills/firecrawl-cli/SKILL.md"]);
        for key in OUTPUT_KEYS {
            assert!(!out[*key], ".agents changes should not enable {key}");
        }
    }

    #[test]
    fn prose_docs_skip_runtime_but_enable_soma_docs() {
        let out = classify_paths(&["docs/SCAFFOLD.md"]);
        assert!(out["docs"]);
        assert!(out["soma"]);
        assert!(!out["rust"]);
        assert!(!out["web"]);
        assert!(!out["native"]);
        assert!(!out["docker"]);
    }

    #[test]
    fn rust_changes_enable_runtime_dependents() {
        let out = classify_paths(&["crates/soma/mcp/src/tool.rs"]);
        assert!(out["rust"]);
        assert!(out["mcp"]);
        assert!(out["native"]);
        assert!(out["docker"]);
        assert!(out["soma"]);
        assert!(out["security"]);
        assert!(out["release"]);
        assert!(
            !out["docker_build"],
            "rust-only changes should not force a PR container image rebuild"
        );
        assert!(!out["python"]);
        assert!(!out["wasm_component"]);
    }

    #[test]
    fn dockerfile_changes_enable_docker_build() {
        for path in [
            "config/Dockerfile",
            ".dockerignore",
            ".env.example",
            "docker-compose.yml",
            "docker-compose.prod.yml",
        ] {
            let out = classify_paths(&[path]);
            assert!(out["docker_build"], "{path} should enable docker_build");
            assert!(out["docker"], "{path} should enable docker");
        }
    }

    #[test]
    fn python_package_changes_enable_python() {
        let out = classify_paths(&["packages/python/python/soma_provider/runner.py"]);
        assert!(out["python"]);
        assert!(!out["rust"]);
        assert!(!out["wasm_component"]);
    }

    #[test]
    fn component_example_changes_enable_wasm_component() {
        let out = classify_paths(&["examples/providers/components/reference-provider/src/lib.rs"]);
        assert!(out["wasm_component"]);
        assert!(!out["python"]);
    }

    #[test]
    fn soma_app_changes_enable_runtime_dependents() {
        let out = classify_paths(&["apps/soma/src/lib.rs"]);
        assert!(out["rust"]);
        assert!(out["native"]);
        assert!(out["docker"]);
        assert!(out["soma"]);
        assert!(out["security"]);
        assert!(out["release"]);
    }

    #[test]
    fn npm_launcher_changes_enable_release_checks() {
        let out = classify_paths(&["packages/soma-rmcp/scripts/install.js"]);
        assert!(out["release"]);
        assert!(!out["rust"]);
        assert!(!out["web"]);
    }

    #[test]
    fn core_workflow_changes_fail_safe_to_full_ci() {
        for path in [
            ".github/workflows/ci.yml",
            ".github/actions/setup-rust-kache/action.yml",
            "xtask/src/ci_paths.rs",
        ] {
            let out = classify_paths(&[path]);
            for key in OUTPUT_KEYS {
                assert!(out[*key], "{path} changes should enable {key}");
            }
        }
    }

    #[test]
    fn non_core_workflow_changes_only_enable_workflow_checks() {
        let out = classify_paths(&[".github/workflows/release.yml"]);
        assert!(out["workflow"]);
        assert!(out["secrets"]);
        assert!(!out["rust"], "non-core workflow edits must not run rust CI");
        assert!(!out["docker"]);
        assert!(!out["docker_build"]);
        assert!(!out["web"]);
        assert!(!out["mcp"]);
        assert!(!out["soma"]);
        assert!(!out["release"]);
        assert!(!out["all"]);
    }

    #[test]
    fn manual_runs_enable_full_ci() {
        let out = classify("workflow_dispatch", &[]);
        for key in OUTPUT_KEYS {
            assert!(out[*key], "workflow_dispatch should enable {key}");
        }
    }
}
