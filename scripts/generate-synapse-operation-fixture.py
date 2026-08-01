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
DONOR_REPOSITORY = "https://github.com/dinglebear-ai/synapse"
EXPECTED_COUNT = 59
FORMAT_VERSION = 2

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

SCOPE_VALUES = {
    None: None,
    "READ_SCOPE": "synapse:read",
    "WRITE_SCOPE": "synapse:write",
}

ACCESS_VALUES = {
    None: "public",
    "READ_SCOPE": "read",
    "WRITE_SCOPE": "write",
}


def run_git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return completed.stdout


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def semantic_digest(operations: list[dict[str, Any]]) -> str:
    encoded = json.dumps(
        operations,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    )
    return sha256_text(encoded)


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
        legacy_name = match.group("name")
        normalized_line = line.strip()
        operations.append(
            {
                "legacy_name": legacy_name,
                "canonical_name": canonical_name(legacy_name),
                "legacy_tool": match.group("tool").lower(),
                "legacy_action": match.group("action"),
                "legacy_subaction": match.group("sub"),
                "legacy_access": ACCESS_VALUES[scope_name],
                "legacy_scope": SCOPE_VALUES[scope_name],
                "legacy_destructive": match.group("destructive") == "true",
                "legacy_transport": (
                    "rest" if match.group("transport") == "Rest" else "mcp_only"
                ),
                "required_params": strings(match.group("required")),
                "required_any": alternatives(match.group("required_any")),
                "source_path": SOURCE_PATH,
                "source_line": number,
                "source_macro_sha256": sha256_text(normalized_line),
            }
        )
    return operations


def build_fixture(repo: Path, ref: str) -> dict[str, Any]:
    commit = run_git(repo, "rev-parse", ref).strip()
    source = run_git(repo, "show", f"{ref}:{SOURCE_PATH}")
    operations = parse_source(source)
    fixture = {
        "format_version": FORMAT_VERSION,
        "donor": {
            "repository": DONOR_REPOSITORY,
            "commit": commit,
            "source_path": SOURCE_PATH,
            "source_sha256": sha256_text(source),
        },
        "operation_count": len(operations),
        "semantic_sha256": semantic_digest(operations),
        "operations": operations,
    }
    validate_fixture(fixture)
    return fixture


def validate_string_list(value: Any, label: str, *, allow_empty: bool = True) -> None:
    if not isinstance(value, list):
        raise ValueError(f"{label} must be an array")
    if not allow_empty and not value:
        raise ValueError(f"{label} must not be empty")
    if any(not isinstance(item, str) or not item for item in value):
        raise ValueError(f"{label} must contain non-empty strings")
    if len(set(value)) != len(value):
        raise ValueError(f"{label} contains duplicates")


def validate_fixture(fixture: dict[str, Any]) -> None:
    if fixture.get("format_version") != FORMAT_VERSION:
        raise ValueError(f"format_version must be {FORMAT_VERSION}")

    operations = fixture.get("operations")
    if not isinstance(operations, list):
        raise ValueError("operations must be an array")
    if fixture.get("operation_count") != len(operations):
        raise ValueError("operation_count does not match operations length")
    if len(operations) != EXPECTED_COUNT:
        raise ValueError(f"expected {EXPECTED_COUNT} operations, found {len(operations)}")
    if fixture.get("semantic_sha256") != semantic_digest(operations):
        raise ValueError("semantic_sha256 does not match operation semantics")

    legacy = [item.get("legacy_name") for item in operations]
    canonical = [item.get("canonical_name") for item in operations]
    if len(set(legacy)) != len(legacy):
        raise ValueError("legacy operation names are not unique")
    if len(set(canonical)) != len(canonical):
        raise ValueError("canonical operation names are not unique")

    source_lines: list[int] = []
    shapes: set[tuple[str, str, str | None]] = set()
    for item in operations:
        name = str(item.get("legacy_name"))
        expected = canonical_name(name)
        if item.get("canonical_name") != expected:
            raise ValueError(
                f"canonical mapping drift for {name}: expected {expected}, "
                f"found {item.get('canonical_name')}"
            )

        tool = item.get("legacy_tool")
        action = item.get("legacy_action")
        subaction = item.get("legacy_subaction")
        if tool not in {"flux", "scout", "both"}:
            raise ValueError(f"invalid legacy_tool for {name}")
        if not isinstance(action, str) or not action:
            raise ValueError(f"invalid legacy_action for {name}")
        if subaction is not None and (not isinstance(subaction, str) or not subaction):
            raise ValueError(f"invalid legacy_subaction for {name}")
        shape = (str(tool), action, subaction)
        if shape in shapes:
            raise ValueError(f"duplicate legacy operation shape for {name}: {shape}")
        shapes.add(shape)

        access = item.get("legacy_access")
        scope = item.get("legacy_scope")
        expected_scope = {
            "public": None,
            "read": "synapse:read",
            "write": "synapse:write",
        }.get(access)
        if access not in {"public", "read", "write"} or scope != expected_scope:
            raise ValueError(f"invalid access/scope binding for {name}")
        if item.get("legacy_destructive") and access != "write":
            raise ValueError(f"destructive operation {name} must require write access")
        if item.get("legacy_transport") not in {"rest", "mcp_only"}:
            raise ValueError(f"invalid transport for {name}")

        required = item.get("required_params")
        validate_string_list(required, f"required_params for {name}")
        required_any = item.get("required_any")
        if not isinstance(required_any, list):
            raise ValueError(f"required_any for {name} must be an array")
        normalized_groups: set[tuple[str, ...]] = set()
        for index, group in enumerate(required_any):
            validate_string_list(
                group,
                f"required_any[{index}] for {name}",
                allow_empty=False,
            )
            normalized = tuple(group)
            if normalized in normalized_groups:
                raise ValueError(f"duplicate required_any group for {name}")
            normalized_groups.add(normalized)

        if item.get("source_path") != SOURCE_PATH:
            raise ValueError(f"invalid source_path for {name}")
        source_line = item.get("source_line")
        if not isinstance(source_line, int) or source_line <= 0:
            raise ValueError(f"invalid source_line for {name}")
        source_lines.append(source_line)
        if not re.fullmatch(
            r"[0-9a-f]{64}", str(item.get("source_macro_sha256", ""))
        ):
            raise ValueError(f"invalid source macro digest for {name}")

    if source_lines != sorted(source_lines) or len(set(source_lines)) != len(source_lines):
        raise ValueError("source lines must be unique and ordered")

    donor = fixture.get("donor", {})
    if donor.get("repository") != DONOR_REPOSITORY:
        raise ValueError("donor repository is not canonical")
    if donor.get("source_path") != SOURCE_PATH:
        raise ValueError("donor source_path is invalid")
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
            print(
                f"wrote {args.fixture} with {fixture['operation_count']} operations "
                f"({fixture['semantic_sha256'][:12]})"
            )
            return 0

        committed = load_fixture(args.fixture)
        validate_fixture(committed)
        if args.donor_repo is not None:
            generated = build_fixture(args.donor_repo, args.ref)
            if committed != generated:
                raise ValueError(
                    "committed Synapse operation fixture differs from the pinned donor source"
                )
        print(
            f"Synapse operation fixture is valid ({EXPECTED_COUNT} operations, "
            f"{committed['semantic_sha256'][:12]})"
        )
        return 0
    except (OSError, subprocess.CalledProcessError, ValueError, json.JSONDecodeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
