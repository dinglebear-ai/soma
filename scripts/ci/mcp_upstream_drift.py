#!/usr/bin/env python3
"""Report MCP specification/rmcp drift and map it to Soma ownership."""

from __future__ import annotations

import argparse
import json
import os
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class Ownership:
    keywords: tuple[str, ...]
    local_paths: tuple[str, ...]
    checks: tuple[str, ...]


OWNERSHIP = (
    Ownership(
        ("authorization", "oauth", "security-considerations", "client-registration"),
        ("crates/shared/auth/src/", "apps/soma/src/http.rs"),
        ("cargo test -p soma-auth --all-targets",),
    ),
    Ownership(
        ("transport", "streamable-http", "stateless", "session"),
        ("apps/soma/src/http.rs", "crates/shared/mcp/server/src/", "crates/shared/mcp/client/src/"),
        ("cargo test -p soma -p soma-mcp-client --all-targets",),
    ),
    Ownership(
        ("discover", "lifecycle", "versioning", "initialize"),
        ("crates/shared/mcp/client/src/upstream/pool/lifecycle_compat.rs", "crates/soma/mcp/src/rmcp_server.rs"),
        ("cargo test -p soma-mcp-client lifecycle_compat",),
    ),
    Ownership(
        ("task", "mrtr", "elicitation", "input_required"),
        ("crates/soma/mcp/src/rmcp_server/", "crates/shared/mcp/gateway/src/", "crates/shared/mcp/client/src/upstream/pool/"),
        ("cargo test -p soma-mcp -p soma-mcp-client --all-targets",),
    ),
    Ownership(
        ("tool", "resource", "prompt", "completion", "schema"),
        ("crates/soma/mcp/src/rmcp_server.rs", "crates/soma/mcp/src/gateway_proxy.rs"),
        ("cargo test -p soma-mcp --all-targets",),
    ),
    Ownership(
        ("caching", "subscription", "notification", "event-store"),
        ("crates/soma/mcp/src/rmcp_server/catalog_subscriptions.rs", "crates/soma/mcp/src/rmcp_server.rs"),
        ("cargo test -p soma-mcp catalog_subscriptions",),
    ),
    Ownership(
        ("extension", "apps", "ui"),
        ("crates/soma/mcp/src/rmcp_server.rs", "apps/soma/src/http.rs"),
        ("scripts/ci/mcp-conformance.sh",),
    ),
)

DEFAULT_PATHS = (
    "Cargo.toml",
    "scripts/ci/mcp-conformance.sh",
    "docs/specs/mcp-draft-2026-07-28-migration.md",
)
DEFAULT_CHECKS = ("scripts/ci/mcp-conformance.sh",)


def api_json(url: str, token: str | None) -> Any:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "soma-mcp-drift-watch",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"
    with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=30) as response:
        return json.load(response)


def changed_files(compare: dict[str, Any]) -> list[str]:
    return [entry["filename"] for entry in compare.get("files", [])]


def map_ownership(paths: list[str], release_text: str = "") -> tuple[list[str], list[str]]:
    haystack = "\n".join(paths + [release_text]).lower()
    local_paths: set[str] = set(DEFAULT_PATHS)
    checks: set[str] = set(DEFAULT_CHECKS)
    for owner in OWNERSHIP:
        if any(keyword in haystack for keyword in owner.keywords):
            local_paths.update(owner.local_paths)
            checks.update(owner.checks)
    return sorted(local_paths), sorted(checks)


def compare_url(repository: str, before: str, after: str) -> str:
    return f"https://api.github.com/repos/{repository}/compare/{before}...{after}"


def generate_report(baseline: dict[str, Any], token: str | None) -> tuple[str, bool]:
    spec = baseline["mcp_spec"]
    rmcp = baseline["rmcp"]
    spec_head = api_json(
        f"https://api.github.com/repos/{spec['repository']}/commits/{spec['ref']}", token
    )["sha"]
    releases = api_json(
        f"https://api.github.com/repos/{rmcp['repository']}/releases?per_page=20", token
    )
    latest_release = next(release for release in releases if not release["draft"])
    latest_tag = latest_release["tag_name"]
    latest_commit = api_json(
        f"https://api.github.com/repos/{rmcp['repository']}/commits/{latest_tag}", token
    )["sha"]

    spec_compare = api_json(
        compare_url(spec["repository"], spec["commit"], spec_head), token
    )
    rmcp_compare = api_json(
        compare_url(rmcp["repository"], rmcp["commit"], latest_commit), token
    )
    spec_files = changed_files(spec_compare)
    rmcp_files = changed_files(rmcp_compare)
    drift = spec_head != spec["commit"] or latest_commit != rmcp["commit"]
    mapped_paths, checks = map_ownership(
        spec_files + rmcp_files, latest_release.get("body") or ""
    )

    lines = [
        "# MCP upstream drift report",
        "",
        "<!-- soma-mcp-upstream-drift -->",
        "",
        f"**Drift detected:** {'yes' if drift else 'no'}",
        "",
        "## Baselines and current upstream",
        "",
        "| Surface | Baseline | Current |",
        "|---|---|---|",
        f"| MCP spec `{spec['protocol_version']}` | `{spec['commit']}` | `{spec_head}` |",
        f"| rmcp `{rmcp['crate_version']}` | `{rmcp['commit']}` / `{rmcp['release_tag']}` | `{latest_commit}` / `{latest_tag}` |",
        "",
        "## Upstream files changed",
        "",
        "### MCP specification",
        *([f"- `{path}`" for path in spec_files] or ["- None"]),
        "",
        "### rmcp",
        *([f"- `{path}`" for path in rmcp_files] or ["- None"]),
        "",
        "## Soma code that must be reviewed",
        "",
        *[f"- `{path}`" for path in mapped_paths],
        "",
        "## Required validation",
        "",
        *[f"- `{check}`" for check in checks],
        "",
        "When the upstream change is intentionally adopted, update code/tests first, run the",
        "listed validation, then advance `conformance/upstream-baseline.json` in the same PR.",
    ]
    return "\n".join(lines) + "\n", drift


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--baseline", type=Path, default=Path("conformance/upstream-baseline.json")
    )
    parser.add_argument("--output", type=Path, default=Path("target/mcp-upstream-drift.md"))
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    report, drift = generate_report(
        json.loads(args.baseline.read_text()), os.environ.get("GITHUB_TOKEN")
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(report)
    print(report, end="")
    if args.github_output:
        with args.github_output.open("a") as output:
            output.write(f"drift={'true' if drift else 'false'}\n")
            output.write(f"report={args.output}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
