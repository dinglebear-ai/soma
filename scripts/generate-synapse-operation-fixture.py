#!/usr/bin/env python3
"""Generate and validate the pinned Synapse operation compatibility fixture."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE = ROOT / "docs/unify/03-contracts/examples/synapse-operations.json"
SOURCE_PATH = "src/actions/operations.rs"
EXPECTED_COUNT = 59

OPERATION_RE = re.compile(
    r'^\s*operation!\("(?P<name>[^"]+)",\s*'
    r'(?P<tool>\w+),\s*"(?P<action>[^"]+)",\s*'
    r'(?P<subaction>None|Some\("(?P<sub>[^"]+)"\)),\s*'
    r'(?P<scope>None|Some\((?P<scope_name>READ_SCOPE|WRITE_SCOPE)\)),\s*'
    r'(?P<destructive>true|false),\s*'
    r'(?P<transport>Rest|McpOnly),\s*'
    r'\[(?P<required>[^\]]*)\]'
    r'(?:,\s*any\s*\[(?P<required_any>.*)\])?\),$'
)

SCOUT_CANONICAL = {
    "scout.nodes": "fleet.nodes",
    "scout.peek": "files.read",
    "scout.find": "files.find",
    "scout.ps": "processes.list",
    "scout.df": "filesystem.usage",
    "scout.delta": "files.compare",
    "scout.exec": "host.exec",
    "scout.emit": "host.exec_many",
    "scout.beam": "files.transfer",
    "scout.zfs.pools": "zfs.pools",
    "scout.zfs.datasets": "zfs.datasets",
    "scout.zfs.snapshots": "zfs.snapshots",
    "scout.logs.syslog": "logs.syslog",
    "scout.logs.journal": "logs.journal",
    "scout.logs.dmesg": "logs.kernel",
    "scout.logs.auth": "logs.auth",
}


def run_git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return completed.stdout


def strings(value: str | None) -> list[str]:
    return re.findall(r'"([^"]+)"', value or "")


def alternatives(value: str | None) -> list[list[str]]:
    if not value:
        return []
    return [strings(group) for group in re.findall(r'\[([^\]]+)\]', value)]


def canonical_name(legacy: str) -> str:
    if legacy == "help":
        return "product.help"
    if legacy.startswith("flux."):
        return legacy.removeprefix("flux.")
    try:
        return SCOUT_CANONICAL[legacy]
    except KeyError as exc:
        raise ValueError(f"no canonical mapping for {legacy}") from exc


def parse_source(source: str) -> list[dict[str, Any]]:
    operations: list[dict[str, Any]] = []
    for number, line in enumerate(source.splitlines(), start=1):
        if "operation!(" not in line:
            continue
        match = OPERATION_RE.match(line)
        if match is None:
            raise ValueError(f"unparsed operation macro at line {number}: {line}")
        scope_name = match.group("scope_name")
        access = {
            None: "public",
            "READ_SCOPE": "read",
            "WRITE_SCOPE": "write",
        }[scope_name]
        legacy_name = match.group("name")
        operations.append(
            {
                "legacy_name": legacy_name,
                "canonical_name": canonical_name(legacy_name),
                "legacy_tool": match.group("tool").lower(),
                "legacy_action": match.group("action"),
                "legacy_subaction": match.group("sub"),
                "legacy_access": access,
                "legacy_destructive": match.group("destructive") == "true",
                "legacy_transport": (
                    "rest" if match.group("transport") == "Rest" else "mcp_only"
                ),
                "required_params": strings(match.group("required")),
                "required_any": alternatives(match.group("required_any")),
                "source_line": number,
            }
        )
    return operations


def build_fixture(repo: Path, ref: str) -> dict[str, Any]:
    commit = run_git(repo, "rev-parse", ref).strip()
    source = run_git(repo, "show", f"{ref}:{SOURCE_PATH}")
    operations = parse_source(source)
    fixture = {
        "format_version": 1,
        "donor": {
            "repository": "https://github.com/jmagar/synapse",
            "commit": commit,
            "source_path": SOURCE_PATH,
            "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        },
        "operation_count": len(operations),
        "operations": operations,
    }
    validate_fixture(fixture)
    return fixture


def validate_fixture(fixture: dict[str, Any]) -> None:
    operations = fixture.get("operations")
    if not isinstance(operations, list):
        raise ValueError("operations must be an array")
    if fixture.get("operation_count") != len(operations):
        raise ValueError("operation_count does not match operations length")
    if len(operations) != EXPECTED_COUNT:
        raise ValueError(f"expected {EXPECTED_COUNT} operations, found {len(operations)}")

    legacy = [item.get("legacy_name") for item in operations]
    canonical = [item.get("canonical_name") for item in operations]
    if len(set(legacy)) != len(legacy):
        raise ValueError("legacy operation names are not unique")
    if len(set(canonical)) != len(canonical):
        raise ValueError("canonical operation names are not unique")

    for item in operations:
        expected = canonical_name(str(item.get("legacy_name")))
        if item.get("canonical_name") != expected:
            raise ValueError(
                f"canonical mapping drift for {item.get('legacy_name')}: "
                f"expected {expected}, found {item.get('canonical_name')}"
            )
        if item.get("legacy_access") not in {"public", "read", "write"}:
            raise ValueError(f"invalid legacy_access for {item.get('legacy_name')}")
        if item.get("legacy_transport") not in {"rest", "mcp_only"}:
            raise ValueError(f"invalid transport for {item.get('legacy_name')}")

    donor = fixture.get("donor", {})
    if not re.fullmatch(r"[0-9a-f]{40}", str(donor.get("commit", ""))):
        raise ValueError("donor commit must be a full lowercase SHA")
    if not re.fullmatch(r"[0-9a-f]{64}", str(donor.get("source_sha256", ""))):
        raise ValueError("source_sha256 must be a lowercase SHA-256")


def load_fixture(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("generate", "check"))
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--donor-repo", type=Path)
    parser.add_argument("--ref", default="origin/main")
    args = parser.parse_args()

    try:
        if args.action == "generate":
            if args.donor_repo is None:
                parser.error("generate requires --donor-repo")
            fixture = build_fixture(args.donor_repo, args.ref)
            args.fixture.parent.mkdir(parents=True, exist_ok=True)
            args.fixture.write_text(
                json.dumps(fixture, indent=2, ensure_ascii=True) + "\n",
                encoding="utf-8",
            )
            print(f"wrote {args.fixture} with {fixture['operation_count']} operations")
            return 0

        committed = load_fixture(args.fixture)
        validate_fixture(committed)
        if args.donor_repo is not None:
            generated = build_fixture(args.donor_repo, args.ref)
            if committed != generated:
                raise ValueError(
                    "committed Synapse operation fixture differs from the pinned donor source"
                )
        print(f"Synapse operation fixture is valid ({EXPECTED_COUNT} operations)")
        return 0
    except (OSError, subprocess.CalledProcessError, ValueError, json.JSONDecodeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
