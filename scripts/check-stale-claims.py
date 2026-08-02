#!/usr/bin/env python3
"""Fail on stale Soma claims that should never come back."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TEXT_FORBIDDEN = {
    "localhost:3100": "stale local Soma port; use localhost:40060",
    "default_mcp_port() -> u16 {\n    40000": "stale default MCP port; use 40060",
}
SKIP_PARTS = {
    ".git",
    ".full-review",
    ".worktrees",
    "target",
    "node_modules",
    ".next",
    "out",
    "crates/synapse/import",
    "docs/sessions",
    "docs/references",
}
TEXT_SUFFIXES = {
    ".md",
    ".rs",
    ".py",
    ".sh",
    ".ts",
    ".tsx",
    ".toml",
    ".json",
    ".yml",
    ".yaml",
    ".example",
}
PLUGIN_MANIFESTS = [
    ROOT / "plugins/soma/.claude-plugin/plugin.json",
    ROOT / "plugins/soma/.codex-plugin/plugin.json",
    ROOT / "plugins/soma/gemini-extension.json",
]


def should_skip(path: Path) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    if path == ROOT / "scripts/check-stale-claims.py":
        return True
    parts = set(path.relative_to(ROOT).parts)
    return any(rel == part or rel.startswith(f"{part}/") or part in parts for part in SKIP_PARTS)


def is_text_path(path: Path) -> bool:
    if path.name in {".env.example", "config.soma.toml", "Justfile", "CLAUDE.md", "README.md"}:
        return True
    return path.suffix in TEXT_SUFFIXES


def scan_text() -> list[str]:
    failures: list[str] = []
    for path in ROOT.rglob("*"):
        if not path.is_file() or should_skip(path) or not is_text_path(path):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        rel = path.relative_to(ROOT)
        for needle, reason in TEXT_FORBIDDEN.items():
            if needle in text:
                failures.append(f"{rel}: contains {needle!r} ({reason})")
    return failures


def scan_plugin_versions() -> list[str]:
    failures: list[str] = []
    for path in PLUGIN_MANIFESTS:
        data = json.loads(path.read_text(encoding="utf-8"))
        if "version" in data:
            failures.append(f"{path.relative_to(ROOT)}: plugin manifests must not contain version")
    return failures


def main() -> int:
    failures = scan_text() + scan_plugin_versions()
    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    print("stale claim check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
