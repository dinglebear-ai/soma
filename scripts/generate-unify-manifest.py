#!/usr/bin/env python3
"""Generate or check the docs/unify package manifest and checksums."""

from __future__ import annotations

import argparse
import hashlib
import sys
import tomllib
from pathlib import Path
from typing import Any

try:
    import yaml
except ModuleNotFoundError as exc:
    print(
        "error: PyYAML is required; install it with `python3 -m pip install PyYAML`",
        file=sys.stderr,
    )
    raise SystemExit(1) from exc

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "docs/unify"
MANIFEST = PACKAGE / "MANIFEST.yaml"
CHECKSUMS = PACKAGE / "CHECKSUMS.sha256"
LOCK = PACKAGE / "05-migration/donors.lock.toml"
MANIFEST_EXCLUDES = {"MANIFEST.yaml", "CHECKSUMS.sha256"}
CHECKSUM_EXCLUDES = {"CHECKSUMS.sha256"}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_files(excludes: set[str]) -> list[Path]:
    return sorted(
        path
        for path in PACKAGE.rglob("*")
        if path.is_file() and path.relative_to(PACKAGE).as_posix() not in excludes
    )


def donor_baselines() -> dict[str, dict[str, str]]:
    lock = tomllib.loads(LOCK.read_text(encoding="utf-8"))
    baselines: dict[str, dict[str, str]] = {}
    for name in ("soma", "axon", "cortex", "synapse"):
        donor = lock[name]
        baselines[name] = {
            "repo": donor["repository"],
            "branch": "main",
            "commit": donor["commit"],
            "observed": "2026-07-31",
        }
    return baselines


def file_records() -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for path in package_files(MANIFEST_EXCLUDES):
        relative = path.relative_to(PACKAGE).as_posix()
        records.append(
            {
                "path": relative,
                "bytes": path.stat().st_size,
                "sha256": digest(path),
                "type": path.suffix.removeprefix(".") or "file",
            }
        )
    return records


def manifest_document() -> dict[str, Any]:
    files = file_records()
    return {
        "apiVersion": "soma.dev/v1",
        "kind": "DocumentationPackageManifest",
        "metadata": {
            "name": "soma-product-family-documentation-package",
            "version": "0.2.0-proposed",
            "generatedAt": "2026-07-31",
            "status": "proposed",
        },
        "scope": {
            "includes": [
                "multi-distribution product family",
                "Labby gateway platform",
                "Axon knowledge pipeline",
                "Cortex observations and evidence graph",
                "Synapse operations plane",
                "Soma integrated context and product composition",
            ],
            "excludes": [
                "Agent Package Manager",
                "agent worker deployment",
                "custom Incus image construction",
                "autonomous implementation or deployment in context v1",
            ],
        },
        "baselines": donor_baselines(),
        "counts": {
            "filesInManifest": len(files),
            "sharedCrates": 19,
            "adrs": 13,
            "capabilities": 13,
            "schemaDefinitions": 35,
        },
        "entrypoints": [
            "START-HERE.md",
            "README.md",
            "MASTER-SPEC.md",
            "01-architecture/TARGET-ARCHITECTURE.md",
            "02-crates/CATALOG.md",
            "03-contracts/README.md",
            "05-migration/IMPLEMENTATION-ROADMAP.md",
            "05-migration/SYNAPSE-EXTRACTION.md",
            "06-testing/NORTH-STAR-LABBY-OAUTH.md",
            "VALIDATION-REPORT.md",
        ],
        "files": files,
    }


def render_manifest() -> str:
    return yaml.safe_dump(
        manifest_document(),
        sort_keys=False,
        allow_unicode=False,
        width=100,
    )


def render_checksums(manifest_text: str) -> str:
    lines = []
    for path in package_files(CHECKSUM_EXCLUDES):
        relative = path.relative_to(PACKAGE).as_posix()
        file_digest = (
            hashlib.sha256(manifest_text.encode()).hexdigest()
            if path == MANIFEST
            else digest(path)
        )
        lines.append(f"{file_digest}  {relative}")
    return "\n".join(lines) + "\n"


def generate() -> tuple[str, str]:
    manifest_text = render_manifest()
    MANIFEST.write_text(manifest_text, encoding="utf-8")
    checksums_text = render_checksums(manifest_text)
    CHECKSUMS.write_text(checksums_text, encoding="utf-8")
    return manifest_text, checksums_text


def check() -> None:
    expected_manifest = render_manifest()
    current_manifest = MANIFEST.read_text(encoding="utf-8")
    if current_manifest != expected_manifest:
        raise ValueError(
            "docs/unify/MANIFEST.yaml is stale; run "
            "`python3 scripts/generate-unify-manifest.py generate` "
            "or `just unify-manifest-generate` from the repository root"
        )

    expected_checksums = render_checksums(expected_manifest)
    current_checksums = CHECKSUMS.read_text(encoding="utf-8")
    if current_checksums != expected_checksums:
        raise ValueError(
            "docs/unify/CHECKSUMS.sha256 is stale; run "
            "`python3 scripts/generate-unify-manifest.py generate` "
            "or `just unify-manifest-generate` from the repository root"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("generate", "check"))
    args = parser.parse_args()

    try:
        if args.action == "generate":
            manifest, checksums = generate()
            print(
                f"wrote manifest ({manifest.count(chr(10))} lines) and "
                f"{checksums.count(chr(10))} checksums"
            )
        else:
            check()
            print("docs/unify manifest and checksums are current")
        return 0
    except (OSError, KeyError, ValueError, tomllib.TOMLDecodeError, yaml.YAMLError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
