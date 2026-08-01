#!/usr/bin/env python3
"""Generate a deterministic CycloneDX 1.6 SBOM for release artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import zipfile
from email.parser import Parser
from pathlib import Path
from urllib.parse import quote
from uuid import NAMESPACE_URL, uuid5


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def wheel_metadata(path: Path) -> tuple[str | None, str | None]:
    if path.suffix != ".whl":
        return None, None
    with zipfile.ZipFile(path) as archive:
        names = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
        if len(names) != 1:
            raise ValueError(f"{path} must contain exactly one METADATA file")
        metadata = Parser().parsestr(archive.read(names[0]).decode("utf-8"))
        name = metadata.get("Name")
        version = metadata.get("Version")
        if not name or not version:
            raise ValueError(f"{path} METADATA must contain Name and Version")
        return name, version


def normalized_project_name(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def render(paths: list[Path], root: Path) -> dict[str, object]:
    components = []
    ordered = sorted(paths, key=lambda value: value.relative_to(root).as_posix())
    for path in ordered:
        relative = path.relative_to(root).as_posix()
        name, version = wheel_metadata(path)
        digest = sha256(path)
        component: dict[str, object] = {
            "type": "library" if path.suffix == ".whl" else "file",
            "name": name or path.name,
            "bom-ref": f"sha256:{digest}",
            "hashes": [{"alg": "SHA-256", "content": digest}],
            "properties": [
                {"name": "soma:artifact:path", "value": relative},
                {"name": "soma:artifact:size", "value": str(path.stat().st_size)},
            ],
        }
        if name and version:
            component["version"] = version
            project = quote(normalized_project_name(name), safe="")
            component["purl"] = f"pkg:pypi/{project}@{quote(version, safe='')}"
        components.append(component)
    serial_seed = chr(10).join(str(component["bom-ref"]) for component in components)
    serial = uuid5(NAMESPACE_URL, serial_seed)
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {"component": {"type": "application", "name": "soma-release-artifacts"}},
        "components": components,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    paths = [path for path in args.directory.rglob("*") if path.is_file() and path != args.output]
    if not paths:
        parser.error("artifact directory contains no files")
    document = render(paths, args.directory)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=2, sort_keys=True) + chr(10))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
