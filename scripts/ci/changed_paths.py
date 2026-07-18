#!/usr/bin/env python3
"""Classify changed files into local/CI routing categories for soma."""

from __future__ import annotations

import argparse
import subprocess
from collections.abc import Callable
from pathlib import Path

OUTPUT_KEYS = [
    "all",
    "docs",
    "workflow",
    "rust",
    "web",
    "docker",
    "compose",
    "mcp",
    "release",
    "security",
    "soma",
]


def starts(path: str, *prefixes: str) -> bool:
    return any(path == prefix.rstrip("/") or path.startswith(prefix) for prefix in prefixes)


def any_match(paths: list[str], predicate: Callable[[str], bool]) -> bool:
    return any(predicate(path) for path in paths)


def classify(event: str, paths: list[str]) -> dict[str, bool]:
    if event in {"schedule", "workflow_dispatch"} or not paths:
        return {key: True for key in OUTPUT_KEYS}

    workflow = any_match(
        paths,
        lambda p: starts(p, ".github/workflows/", "scripts/ci/")
        or p in {"lefthook.yml", "scripts/check_lefthook_pre_commit_speed.py"},
    )
    docs = any_match(paths, lambda p: starts(p, "docs/") or p in {"README.md", "CHANGELOG.md"})
    rust = any_match(
        paths,
        lambda p: starts(p, "apps/soma/", "crates/", "xtask/", "tests/", ".cargo/", ".config/")
        or p in {"Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "Justfile"},
    )
    web = any_match(paths, lambda p: starts(p, "apps/web/", "crates/soma/web/"))
    compose = any_match(
        paths,
        lambda p: starts(p, "config/")
        or p in {".dockerignore", ".env.example", "docker-compose.yml", "docker-compose.prod.yml"},
    )
    docker = compose or web or any_match(paths, lambda p: p in {".dockerignore", "config/Dockerfile"})
    mcp = any_match(
        paths,
        lambda p: starts(
            p,
            "crates/soma/mcp/",
            "crates/soma/api/",
            # crates/soma/contracts was split (plan PR 13) and deleted
            # (PR 19); the pieces that shape MCP tool schemas and server
            # startup env now live in soma-domain (ACTION_SPECS) and
            # soma-config (McpConfig, env prefixes).
            "crates/soma/domain/",
            "crates/soma/config/",
            "apps/soma/tests/mcporter/",
            "docs/reference/mcp/",
            "docs/generated/",
            "docs/MCP",
        ),
    )
    release = rust or web or any_match(paths, lambda p: starts(p, "release/") or p in {"server.json"})
    security = rust or any_match(paths, lambda p: p in {"Cargo.lock", "deny.toml"} or starts(p, ".cargo/"))
    soma = rust or mcp or docs or any_match(
        paths,
        lambda p: starts(p, "scaffold/", "plugins/", "scripts/")
        or p
        in {
            "cargo-generate.toml",
            "lefthook.yml",
            "Justfile",
            "CLAUDE.md",
            "AGENTS.md",
            "GEMINI.md",
        },
    )

    result = {
        "all": False,
        "docs": docs,
        "workflow": workflow,
        "rust": rust,
        "web": web,
        "docker": docker,
        "compose": compose,
        "mcp": mcp,
        "release": release,
        "security": security,
        "soma": soma,
    }
    return result


def git_path_exists(rev: str) -> bool:
    return (
        subprocess.run(
            ["git", "cat-file", "-e", rev],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        ).returncode
        == 0
    )


def git_output(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True, stderr=subprocess.DEVNULL).strip()


def resolve_paths(event: str) -> list[str]:
    if event in {"schedule", "workflow_dispatch"}:
        return []

    env = __import__("os").environ
    head = env.get("HEAD_SHA") or env.get("GITHUB_SHA") or "HEAD"
    base = ""
    if event == "pull_request":
        base = env.get("PR_BASE_SHA", "")
        head = env.get("PR_HEAD_SHA") or head
    elif event == "push":
        if env.get("GITHUB_REF", "").startswith("refs/tags/"):
            return []
        base = env.get("PUSH_BEFORE_SHA", "")

    if not base or set(base) == {"0"} or not git_path_exists(base):
        try:
            base = git_output("rev-parse", "HEAD^")
        except subprocess.CalledProcessError:
            base = ""

    if not base:
        return []

    try:
        raw = git_output("diff", "--name-only", base, head)
    except subprocess.CalledProcessError:
        return []
    return [line.strip() for line in raw.splitlines() if line.strip()]


def write_outputs(path: Path, values: dict[str, bool]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(f"{key}={'true' if values[key] else 'false'}" for key in OUTPUT_KEYS) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--event", required=True)
    parser.add_argument("--changed-files", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--write-changed-files", type=Path)
    args = parser.parse_args()

    if args.changed_files and args.changed_files.exists():
        paths = [line.strip() for line in args.changed_files.read_text().splitlines() if line.strip()]
    else:
        paths = resolve_paths(args.event)

    if args.write_changed_files:
        args.write_changed_files.write_text("\n".join(paths) + ("\n" if paths else ""))

    values = classify(args.event, paths)
    write_outputs(args.output, values)
    for key in OUTPUT_KEYS:
        print(f"{key}={str(values[key]).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
