#!/usr/bin/env python3
"""Path-aware local pre-push checks for soma."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

import changed_paths

ROOT = Path(__file__).resolve().parents[2]


def truthy(value: str | None) -> bool:
    return value is not None and value.lower() in {"1", "true", "yes", "on"}


def run_git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True, stderr=subprocess.DEVNULL).strip()


def git_ref_exists(ref: str) -> bool:
    return (
        subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", ref],
            cwd=ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        ).returncode
        == 0
    )


def resolve_base() -> str:
    override = os.environ.get("SOMA_PRE_PUSH_BASE")
    if override:
        return override
    for candidate in ("@{upstream}", "origin/main"):
        if not git_ref_exists(candidate):
            continue
        try:
            return run_git("merge-base", candidate, "HEAD")
        except subprocess.CalledProcessError:
            continue
    try:
        return run_git("rev-parse", "HEAD^")
    except subprocess.CalledProcessError:
        return ""


def changed_files(base: str) -> list[str] | None:
    if not base:
        return None
    try:
        raw = run_git("diff", "--name-only", base, "HEAD")
    except subprocess.CalledProcessError:
        return None
    return [line.strip() for line in raw.splitlines() if line.strip()]


def any_path(paths: list[str], *prefixes: str) -> bool:
    return any(path == prefix.rstrip("/") or path.startswith(prefix) for path in paths for prefix in prefixes)


def any_file(paths: list[str], *names: str) -> bool:
    wanted = set(names)
    return any(path in wanted for path in paths)


def command_plan(paths: list[str], categories: dict[str, bool], full: bool) -> list[tuple[str, str]]:
    plan: list[tuple[str, str]] = [
        ("version-sync", "cargo xtask check-version-sync"),
    ]

    workflow_changed = categories["workflow"]
    if workflow_changed:
        plan.extend(
            [
                ("python-syntax", "python3 -m py_compile scripts/ci/changed_paths.py scripts/ci/pre_push.py scripts/check_lefthook_pre_commit_speed.py"),
                ("lefthook-speed", "python3 scripts/check_lefthook_pre_commit_speed.py"),
            ]
        )
        if command_exists("actionlint"):
            plan.append(("workflow-lint", "actionlint .github/workflows/*.yml"))

    if categories["soma"]:
        plan.extend(
            [
                ("coupled-files", "cargo xtask check-coupled-files origin/main HEAD"),
            ]
        )

    if categories["rust"]:
        plan.extend(
            [
                ("architecture", "cargo xtask check-architecture"),
                ("clippy", "cargo clippy --all-targets -- -D warnings"),
            ]
        )

    if categories["web"]:
        plan.append(("web-check", "pnpm -C apps/web validate"))

    if full:
        plan.extend(
            [
                ("python-tests", "just test-python"),
                ("python-package", "just test-python-package"),
            ]
        )
        plan.append(("full-nextest", "cargo nextest run --profile ci"))
    elif categories["rust"] or categories["mcp"]:
        plan.append(("focused-nextest", "cargo nextest run --profile ci --lib --bins --tests"))

    if categories["mcp"]:
        plan.append(("schema-docs", "cargo xtask check-schema-docs --check"))

    if full or categories["release"]:
        plan.append(("release-versions", "cargo xtask check-release-versions --base origin/main --head HEAD --mode pr"))

    if full:
        plan.append(("soma-features", "cargo xtask test-soma-features"))

    return dedupe_plan(plan)


def command_exists(name: str) -> bool:
    return (
        subprocess.run(
            ["bash", "-lc", f"command -v {name} >/dev/null 2>&1"],
            cwd=ROOT,
            check=False,
        ).returncode
        == 0
    )


def dedupe_plan(plan: list[tuple[str, str]]) -> list[tuple[str, str]]:
    seen: set[str] = set()
    out: list[tuple[str, str]] = []
    for name, command in plan:
        if name in seen:
            continue
        seen.add(name)
        out.append((name, command))
    return out


def run_command(name: str, command: str) -> None:
    print(f"\n==> {name}\n{command}", flush=True)
    subprocess.run(["bash", "-lc", command], cwd=ROOT, env=os.environ.copy(), check=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    full = truthy(os.environ.get("SOMA_FULL_PRE_PUSH"))
    base = resolve_base()
    paths = changed_files(base)
    if paths is None:
        if not full:
            print(
                "pre-push: could not determine changed files; running minimal checks only. "
                "Set SOMA_FULL_PRE_PUSH=1 for full local validation.",
                file=sys.stderr,
            )
        paths = []

    if full:
        categories = {key: True for key in changed_paths.OUTPUT_KEYS}
    elif paths:
        categories = changed_paths.classify("pull_request", paths)
    else:
        categories = {key: False for key in changed_paths.OUTPUT_KEYS}
    plan = command_plan(paths, categories, full)

    print("Changed files:")
    if paths:
        for path in paths:
            print(f"  {path}")
    else:
        print("  <none relative to selected base>")
    print("Enabled categories: " + ", ".join(key for key, value in categories.items() if value))
    print("Pre-push plan:")
    for name, command in plan:
        print(f"  {name}: {command}")

    if args.dry_run:
        return 0

    for name, command in plan:
        run_command(name, command)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as exc:
        raise SystemExit(exc.returncode)
