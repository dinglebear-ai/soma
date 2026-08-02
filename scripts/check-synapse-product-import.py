#!/usr/bin/env python3
"""Verify the locked Synapse donor import snapshot boundary."""
from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "docs/unify/05-migration/donors.lock.toml"
PREFIX = Path("crates/synapse/import")
EXPECTED_PACKAGES = ["synapse", "xtask"]
EXPECTED_FILES = 386


def git(*args: str, check: bool = True) -> str:
    completed = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        check=check,
        capture_output=True,
        text=True,
    )
    return completed.stdout


def tree(commit: str, prefix: Path | None = None) -> dict[str, tuple[str, str, str]]:
    args = ["ls-tree", "-r", "--full-tree", commit]
    if prefix is not None:
        args.extend(["--", prefix.as_posix()])
    records: dict[str, tuple[str, str, str]] = {}
    for line in git(*args).splitlines():
        metadata, path = line.split("	", 1)
        mode, kind, object_id = metadata.split()
        if prefix is not None:
            expected = prefix.as_posix() + "/"
            if not path.startswith(expected):
                raise ValueError(f"unexpected imported path {path}")
            path = path.removeprefix(expected)
        records[path] = (mode, kind, object_id)
    return records


def cargo_metadata(manifest: Path) -> dict:
    completed = subprocess.run(
        [
            "cargo",
            "metadata",
            "--manifest-path",
            str(manifest),
            "--no-deps",
            "--format-version",
            "1",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(completed.stdout)


def main() -> int:
    try:
        locked = tomllib.loads(LOCK.read_text(encoding="utf-8"))["synapse"]
        donor = str(locked["commit"])
        expected_import_tree = str(locked["import_tree"])
        repository = str(locked["repository"])
        if repository != "https://github.com/dinglebear-ai/synapse":
            raise ValueError(f"unexpected Synapse donor repository {repository}")

        imported_tree_id = git("rev-parse", f"HEAD:{PREFIX.as_posix()}").strip()
        if imported_tree_id != expected_import_tree:
            raise ValueError(
                "import differs from locked snapshot: "
                f"expected tree {expected_import_tree}, found {imported_tree_id}"
            )
        imported_tree = tree("HEAD", PREFIX)
        if len(imported_tree) != EXPECTED_FILES:
            raise ValueError(
                f"expected {EXPECTED_FILES} imported files, found {len(imported_tree)}"
            )

        nested = cargo_metadata(ROOT / PREFIX / "Cargo.toml")
        package_names = sorted(package["name"] for package in nested["packages"])
        if package_names != EXPECTED_PACKAGES:
            raise ValueError(
                f"unexpected nested packages: expected {EXPECTED_PACKAGES}, found {package_names}"
            )
        if Path(nested["workspace_root"]) != ROOT / PREFIX:
            raise ValueError("Synapse import is not isolated as its own nested workspace")

        soma = cargo_metadata(ROOT / "Cargo.toml")
        soma_names = {package["name"] for package in soma["packages"]}
        if "synapse" in soma_names or "synapse2" in soma_names:
            raise ValueError("temporary Synapse import leaked into the Soma root workspace")

        print(
            "Synapse product import matches locked snapshot "
            f"(donor={donor[:12]}, tree={imported_tree_id[:12]}, "
            f"{len(imported_tree)} files, packages={package_names})"
        )
        return 0
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
