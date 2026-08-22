#!/usr/bin/env python3
"""Validate the Soma agent-runtime documentation package.

Requires PyYAML, jsonschema, and referencing. The repository's mise Python
currently provides these dependencies.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter
from pathlib import Path
from urllib.parse import unquote

try:
    import yaml
    from jsonschema import Draft202012Validator, FormatChecker
    from referencing import Registry, Resource
except ImportError as error:  # pragma: no cover - environment diagnostic
    raise SystemExit(
        "check-agent-runtime-docs requires PyYAML, jsonschema, and referencing: "
        f"{error}"
    ) from error

REPO = Path(__file__).resolve().parents[1]
DOC_ROOT = REPO / "docs" / "agent-runtime"
SCHEMA_ROOT = DOC_ROOT / "schemas"
EXAMPLE_ROOT = DOC_ROOT / "examples"
MANIFEST_PATH = DOC_ROOT / "MANIFEST.yaml"

REQUIRED_FRONTMATTER = {
    "title",
    "created",
    "updated",
    "doc_type",
    "status",
    "owner",
    "scope",
}

EXAMPLE_SCHEMAS = {
    "soma.stack.yaml": "agent-stack.schema.json",
    "soma.context.yaml": "context-manifest.schema.json",
    "read-only.loadout.yaml": "labby-loadout.schema.json",
    "trace-service-failure.snippet.md": "snippet.schema.json",
    "compiled-context.json": "compiled-context.schema.json",
    "run-manifest.json": "agent-run.schema.json",
    "synthesis-result.json": "synthesis-result.schema.json",
}

BASELINES = {
    "soma": "c604d0d503068a64d95d59fcd70e60d6fadf571b",
    "axon": "488684fc90e0726f79efeda5e8e3e07d2cb8981f",
    "cortex": "6afa01ad46594f9ad0e7bd519cdbc44b46664002",
    "labby": "59699f459cc4a68ef72c23200d74fa67d040c474",
    "apm": "dcbaf654cf6de26bb845927d383dd2e2ef9cb723",
}

LINK_RE = re.compile(r"\[[^]]+\]\(([^)]+)\)")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def load_yaml(path: Path):
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def split_frontmatter(path: Path) -> tuple[dict, list[str]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "---":
        raise ValueError("missing YAML frontmatter")
    try:
        end = lines.index("---", 1)
    except ValueError as error:
        raise ValueError("unterminated YAML frontmatter") from error
    frontmatter = yaml.safe_load(chr(10).join(lines[1:end]))
    if not isinstance(frontmatter, dict):
        raise ValueError("frontmatter must be an object")
    return frontmatter, lines[end + 1 :]


def load_snippet(path: Path):
    frontmatter, body = split_frontmatter(path)
    try:
        code_start = body.index("~~~js") + 1
        code_end = body.index("~~~", code_start)
    except ValueError as error:
        raise ValueError("snippet must contain exactly one fenced JavaScript body") from error
    if body[code_end + 1 :].count("~~~js") or body[: code_start - 1].count("~~~js"):
        raise ValueError("snippet must contain exactly one JavaScript body")
    frontmatter.setdefault("spec", {})["code"] = chr(10).join(body[code_start:code_end])
    return frontmatter


def load_example(path: Path):
    if path.name.endswith(".snippet.md"):
        return load_snippet(path)
    if path.suffix == ".json":
        return load_json(path)
    return load_yaml(path)


def schema_registry() -> tuple[dict[str, dict], Registry]:
    schemas = {
        path.name: load_json(path)
        for path in sorted(SCHEMA_ROOT.glob("*.schema.json"))
    }
    registry = Registry()
    for name, schema in schemas.items():
        Draft202012Validator.check_schema(schema)
        schema_id = schema.get("$id")
        if not schema_id:
            raise ValueError(f"schema lacks $id: {name}")
        registry = registry.with_resource(schema_id, Resource.from_contents(schema))
    return schemas, registry


def validate_examples(errors: list[str]) -> None:
    try:
        schemas, registry = schema_registry()
    except Exception as error:  # noqa: BLE001 - aggregate all diagnostics
        errors.append(f"schema registry: {error}")
        return

    for example_name, schema_name in EXAMPLE_SCHEMAS.items():
        path = EXAMPLE_ROOT / example_name
        if not path.is_file():
            errors.append(f"missing example: {path.relative_to(REPO)}")
            continue
        try:
            instance = load_example(path)
            validator = Draft202012Validator(
                schemas[schema_name],
                registry=registry,
                format_checker=FormatChecker(),
            )
            validation_errors = sorted(
                validator.iter_errors(instance),
                key=lambda item: list(item.absolute_path),
            )
            for error in validation_errors:
                location = ".".join(str(part) for part in error.absolute_path) or "<root>"
                errors.append(f"{path.relative_to(REPO)}:{location}: {error.message}")
        except Exception as error:  # noqa: BLE001 - aggregate all diagnostics
            errors.append(f"{path.relative_to(REPO)}: {error}")


def validate_frontmatter(errors: list[str]) -> None:
    for path in sorted(DOC_ROOT.rglob("*.md")):
        if path.name.endswith(".snippet.md"):
            continue
        try:
            frontmatter, _ = split_frontmatter(path)
            missing = sorted(REQUIRED_FRONTMATTER - frontmatter.keys())
            if missing:
                errors.append(
                    f"{path.relative_to(REPO)}: missing frontmatter keys {', '.join(missing)}"
                )
        except Exception as error:  # noqa: BLE001
            errors.append(f"{path.relative_to(REPO)}: {error}")


def validate_links(errors: list[str]) -> None:
    markdown_paths = [REPO / "docs" / "AGENT-RUNTIME.md", *sorted(DOC_ROOT.rglob("*.md"))]
    for path in markdown_paths:
        text = path.read_text(encoding="utf-8")
        for raw_target in LINK_RE.findall(text):
            target = raw_target.strip().strip("<>")
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            target = unquote(target.split("#", 1)[0])
            resolved = (path.parent / target).resolve()
            try:
                resolved.relative_to(REPO.resolve())
            except ValueError:
                errors.append(
                    f"{path.relative_to(REPO)}: link escapes repository: {raw_target}"
                )
                continue
            if not resolved.exists():
                errors.append(
                    f"{path.relative_to(REPO)}: missing link target: {raw_target}"
                )


def manifest_entries() -> list[dict]:
    entries = []
    for path in sorted(DOC_ROOT.rglob("*")):
        if not path.is_file() or path == MANIFEST_PATH:
            continue
        entries.append(
            {
                "path": path.relative_to(DOC_ROOT).as_posix(),
                "bytes": path.stat().st_size,
                "sha256": sha256(path),
                "type": path.suffix.lstrip(".") or "file",
            }
        )
    return entries


def render_manifest() -> dict:
    entries = manifest_entries()
    counts = Counter(entry["type"] for entry in entries)
    return {
        "apiVersion": "soma.dev/v1",
        "kind": "DocumentationPackageManifest",
        "metadata": {
            "name": "soma-agent-runtime-documentation-package",
            "version": "0.1.0-proposed",
            "generatedAt": "2026-08-05",
            "status": "proposed",
        },
        "scope": {
            "includes": [
                "context manifests and compiled contexts",
                "progressive disclosure",
                "Code Mode snippets and synthesis",
                "LABBY loadouts",
                "Incus agent runtimes",
                "Codex assistant integration",
                "Cortex lifecycle observability",
                "Axon dependent research",
                "APM package integration",
            ],
            "excludes": [
                "implemented product runtime",
                "resident assistants",
                "multi-service orchestration",
                "remote Incus transport",
            ],
        },
        "baselines": BASELINES,
        "counts": {
            "filesInManifest": len(entries),
            "markdown": counts["md"],
            "json": counts["json"],
            "yaml": counts["yaml"],
        },
        "entrypoints": [
            "README.md",
            "START-HERE.md",
            "OVERVIEW.md",
            "ARCHITECTURE.md",
            "IMPLEMENTATION-PLAN.md",
            "PROGRESS.md",
        ],
        "files": entries,
    }


def write_manifest() -> None:
    MANIFEST_PATH.write_text(
        yaml.safe_dump(render_manifest(), sort_keys=False, width=1000),
        encoding="utf-8",
    )


def validate_manifest(errors: list[str]) -> None:
    if not MANIFEST_PATH.is_file():
        errors.append(f"missing manifest: {MANIFEST_PATH.relative_to(REPO)}")
        return
    try:
        manifest = load_yaml(MANIFEST_PATH)
        expected = render_manifest()
        if manifest != expected:
            errors.append(
                f"{MANIFEST_PATH.relative_to(REPO)} is stale; run "
                "scripts/check-agent-runtime-docs.py --write-manifest"
            )
    except Exception as error:  # noqa: BLE001
        errors.append(f"{MANIFEST_PATH.relative_to(REPO)}: {error}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write-manifest", action="store_true")
    args = parser.parse_args()

    if args.write_manifest:
        write_manifest()

    errors: list[str] = []
    validate_frontmatter(errors)
    validate_links(errors)
    validate_examples(errors)
    validate_manifest(errors)

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(
        f"agent-runtime docs valid: {len(manifest_entries())} files, "
        f"{len(EXAMPLE_SCHEMAS)} schema-backed examples"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
