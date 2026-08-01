"""Fail-closed compatibility and wheel-evidence scan for componentize-py."""

from __future__ import annotations

import ast
import base64
import csv
import hashlib
import io
import os
import stat
import sys
import zipfile
from collections.abc import Iterable
from email.parser import Parser
from pathlib import PurePosixPath
from typing import Literal, TypedDict

COMPONENTIZE_PY_VERSION = "0.25.0"
POLICY_VERSION = "soma-componentize-v1"
MAX_WHEEL_BYTES = 64 * 1024 * 1024
MAX_WHEEL_ENTRIES = 10_000
MAX_WHEEL_ENTRY_BYTES = 64 * 1024 * 1024
MAX_WHEEL_EXPANDED_BYTES = 256 * 1024 * 1024
MAX_METADATA_BYTES = 256 * 1024
_NATIVE_SUFFIXES = (".so", ".pyd", ".dylib")


class ComponentizeFinding(TypedDict):
    code: str
    severity: Literal["error", "warning"]
    message: str
    line: int | None
    subject: str | None


class ComponentizeWheelEvidence(TypedDict):
    path: str
    filename: str
    sha256: str
    distribution: str | None
    version: str | None
    modules: list[str]
    pure_python: bool
    record_verified: bool
    entries: int


class ComponentizeReport(TypedDict):
    schema_version: int
    policy_version: str
    componentize_py_version: str
    experimental: bool
    compatible: bool
    requires_build_validation: bool
    filename: str
    source_sha256: str
    imports: list[str]
    external_imports: list[str]
    import_distributions: dict[str, str]
    wheel_files: list[str]
    wheel_evidence: list[ComponentizeWheelEvidence]
    findings: list[ComponentizeFinding]


_IMPORT_RULES: dict[str, tuple[Literal["error", "warning"], str, str]] = {
    "_thread": ("error", "threading_assumption", "Python threads are unsupported"),
    "threading": ("error", "threading_assumption", "Python threads are unsupported"),
    "multiprocessing": (
        "error",
        "process_assumption",
        "child processes are unsupported",
    ),
    "subprocess": ("error", "process_assumption", "child processes are unsupported"),
    "socket": ("error", "socket_assumption", "ambient sockets are unsupported"),
    "ctypes": ("error", "native_ffi_assumption", "ctypes native FFI is unsupported"),
    "cffi": ("error", "native_ffi_assumption", "cffi native FFI is unsupported"),
    "os": (
        "warning",
        "filesystem_assumption",
        "os access requires manual review and host capability replacement",
    ),
    "pathlib": (
        "warning",
        "filesystem_assumption",
        "path access requires manual review and host capability replacement",
    ),
    "tempfile": (
        "warning",
        "filesystem_assumption",
        "temporary-file access requires manual review",
    ),
    "shutil": (
        "warning",
        "filesystem_assumption",
        "filesystem mutation requires manual review",
    ),
}

_CALL_RULES: dict[str, tuple[Literal["error", "warning"], str, str]] = {
    "__import__": ("error", "dynamic_import", "dynamic imports are unsupported"),
    "importlib.import_module": (
        "error",
        "dynamic_import",
        "dynamic imports are unsupported",
    ),
    "open": (
        "warning",
        "filesystem_assumption",
        "direct file access requires manual rewrite",
    ),
    "os.system": ("error", "process_assumption", "shell execution is unsupported"),
    "os.fork": ("error", "process_assumption", "process forking is unsupported"),
    "socket.socket": ("error", "socket_assumption", "ambient sockets are unsupported"),
    "threading.Thread": ("error", "threading_assumption", "Python threads are unsupported"),
    "multiprocessing.Process": (
        "error",
        "process_assumption",
        "child processes are unsupported",
    ),
}


class _SourceVisitor(ast.NodeVisitor):
    def __init__(self) -> None:
        self.imports: set[str] = set()
        self.findings: list[ComponentizeFinding] = []
        self.aliases: dict[str, str] = {}

    def visit_Import(self, node: ast.Import) -> None:
        for alias in node.names:
            root = alias.name.split(".", 1)[0]
            self.imports.add(root)
            self.aliases[alias.asname or root] = alias.name
            self._record_import(root, node.lineno)
        self.generic_visit(node)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        if node.module:
            root = node.module.split(".", 1)[0]
            self.imports.add(root)
            self._record_import(root, node.lineno)
            for alias in node.names:
                self.aliases[alias.asname or alias.name] = f"{node.module}.{alias.name}"
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self.findings.append(
            _finding(
                "async_runtime_assumption",
                "error",
                "async provider functions require an event loop unavailable in the component path",
                node.lineno,
                node.name,
            )
        )
        self.generic_visit(node)

    def visit_Call(self, node: ast.Call) -> None:
        name = self._call_name(node.func)
        rule = _CALL_RULES.get(name)
        if rule is not None:
            severity, code, message = rule
            self.findings.append(_finding(code, severity, message, node.lineno, name))
        self.generic_visit(node)

    def _record_import(self, root: str, line: int) -> None:
        rule = _IMPORT_RULES.get(root)
        if rule is not None:
            severity, code, message = rule
            self.findings.append(_finding(code, severity, message, line, root))

    def _call_name(self, node: ast.expr) -> str:
        parts: list[str] = []
        current: ast.expr = node
        while isinstance(current, ast.Attribute):
            parts.append(current.attr)
            current = current.value
        if isinstance(current, ast.Name):
            parts.append(current.id)
        if not parts:
            return ""
        parts.reverse()
        resolved = self.aliases.get(parts[0], parts[0])
        return ".".join([resolved, *parts[1:]])


def scan_componentize_compatibility(
    source: str,
    *,
    filename: str = "<provider>",
    wheel_files: Iterable[str] = (),
) -> ComponentizeReport:
    """Return a digest-bound report without importing provider code."""

    wheels = sorted({os.path.abspath(os.fspath(value)) for value in wheel_files})
    visitor = _SourceVisitor()
    findings: list[ComponentizeFinding]
    try:
        tree = ast.parse(source, filename=filename)
    except SyntaxError as error:
        findings = [
            _finding(
                "python_syntax_error",
                "error",
                error.msg,
                error.lineno,
                filename,
            )
        ]
    else:
        visitor.visit(tree)
        findings = list(visitor.findings)

    external = sorted(
        name
        for name in visitor.imports
        if name not in sys.stdlib_module_names and name != "soma_provider"
    )
    evidence: list[ComponentizeWheelEvidence] = []
    for wheel in wheels:
        item, wheel_findings = _scan_wheel(wheel)
        findings.extend(wheel_findings)
        if item is not None:
            evidence.append(item)

    import_distributions: dict[str, str] = {}
    if external and not wheels:
        findings.append(
            _finding(
                "dependency_wheels_unscanned",
                "error",
                "external imports require explicit, readable dependency wheels",
                None,
                ",".join(external),
            )
        )
    for imported in external:
        matches = [item for item in evidence if imported in item["modules"]]
        if not matches:
            findings.append(
                _finding(
                    "dependency_distribution_missing",
                    "error",
                    "no verified wheel provides this external import",
                    None,
                    imported,
                )
            )
        elif len(matches) > 1:
            findings.append(
                _finding(
                    "dependency_distribution_ambiguous",
                    "error",
                    "multiple wheels claim this external import",
                    None,
                    imported,
                )
            )
        elif matches[0]["distribution"] is not None:
            import_distributions[imported] = matches[0]["distribution"]

    used_distributions = set(import_distributions.values())
    for item in evidence:
        distribution = item["distribution"]
        if distribution is not None and distribution not in used_distributions:
            findings.append(
                _finding(
                    "dependency_wheel_unused",
                    "warning",
                    "verified wheel is not mapped to a provider import",
                    None,
                    item["filename"],
                )
            )

    findings = _deduplicate(findings)
    compatible = not any(item["severity"] == "error" for item in findings)
    return {
        "schema_version": 2,
        "policy_version": POLICY_VERSION,
        "componentize_py_version": COMPONENTIZE_PY_VERSION,
        "experimental": True,
        "compatible": compatible,
        "requires_build_validation": compatible,
        "filename": filename,
        "source_sha256": hashlib.sha256(source.encode("utf-8")).hexdigest(),
        "imports": sorted(visitor.imports),
        "external_imports": external,
        "import_distributions": import_distributions,
        "wheel_files": wheels,
        "wheel_evidence": evidence,
        "findings": findings,
    }


def _scan_wheel(
    path: str,
) -> tuple[ComponentizeWheelEvidence | None, list[ComponentizeFinding]]:
    filename = os.path.basename(path)
    findings: list[ComponentizeFinding] = []
    lower = filename.lower()
    if lower.endswith(_NATIVE_SUFFIXES):
        return None, [
            _finding(
                "native_extension_artifact",
                "error",
                "native extension artifacts are unsupported by componentize-py",
                None,
                filename,
            )
        ]
    if not lower.endswith(".whl"):
        return None, [
            _finding(
                "unsupported_dependency_artifact",
                "error",
                "dependency evidence must be a wheel file",
                None,
                filename,
            )
        ]
    try:
        metadata = os.stat(path)
    except OSError:
        return None, [
            _finding(
                "dependency_wheel_unreadable",
                "error",
                "dependency wheel does not exist or is unreadable",
                None,
                filename,
            )
        ]
    if metadata.st_size > MAX_WHEEL_BYTES:
        return None, [
            _finding(
                "dependency_wheel_too_large",
                "error",
                f"dependency wheel exceeds {MAX_WHEEL_BYTES} bytes",
                None,
                filename,
            )
        ]

    parts = filename[:-4].split("-")
    pure_filename = len(parts) >= 5 and parts[-2].lower() == "none" and parts[-1].lower() == "any"
    if not pure_filename:
        findings.append(
            _finding(
                "native_wheel_unsupported",
                "error",
                "only pure-Python *-none-any wheels are eligible",
                None,
                filename,
            )
        )

    try:
        body = _read_bounded(path, MAX_WHEEL_BYTES)
    except (OSError, ValueError):
        return None, [
            _finding(
                "dependency_wheel_invalid",
                "error",
                "dependency wheel is not a valid bounded ZIP archive",
                None,
                filename,
            )
        ]
    digest = hashlib.sha256(body).hexdigest()
    try:
        archive = zipfile.ZipFile(io.BytesIO(body))
    except zipfile.BadZipFile:
        return (
            {
                "path": path,
                "filename": filename,
                "sha256": digest,
                "distribution": None,
                "version": None,
                "modules": [],
                "pure_python": False,
                "record_verified": False,
                "entries": 0,
            },
            [
                _finding(
                    "dependency_wheel_invalid",
                    "error",
                    "dependency wheel is not a valid bounded ZIP archive",
                    None,
                    filename,
                )
            ],
        )

    with archive:
        infos = archive.infolist()
        if len(infos) > MAX_WHEEL_ENTRIES:
            findings.append(
                _finding(
                    "dependency_wheel_entry_limit",
                    "error",
                    f"dependency wheel exceeds {MAX_WHEEL_ENTRIES} entries",
                    None,
                    filename,
                )
            )
        info_by_name = {info.filename: info for info in infos}
        names = set(info_by_name)
        if len(names) != len(infos):
            findings.append(
                _finding(
                    "dependency_wheel_duplicate_path",
                    "error",
                    "wheel contains duplicate archive paths",
                    None,
                    filename,
                )
            )
        expanded_bytes = sum(info.file_size for info in infos)
        if expanded_bytes > MAX_WHEEL_EXPANDED_BYTES:
            findings.append(
                _finding(
                    "dependency_wheel_expanded_limit",
                    "error",
                    f"wheel expands beyond {MAX_WHEEL_EXPANDED_BYTES} bytes",
                    None,
                    filename,
                )
            )
        for name in sorted(names):
            info = info_by_name[name]
            if info.file_size > MAX_WHEEL_ENTRY_BYTES:
                findings.append(
                    _finding(
                        "dependency_wheel_entry_too_large",
                        "error",
                        f"wheel entry exceeds {MAX_WHEEL_ENTRY_BYTES} bytes",
                        None,
                        name,
                    )
                )
            if info.flag_bits & 0x1:
                findings.append(
                    _finding(
                        "dependency_wheel_encrypted_entry",
                        "error",
                        "encrypted wheel entries are unsupported",
                        None,
                        name,
                    )
                )
            path_parts = PurePosixPath(name).parts
            if not path_parts or name.startswith("/") or ".." in path_parts or chr(0) in name:
                findings.append(
                    _finding(
                        "dependency_wheel_unsafe_path",
                        "error",
                        "wheel contains an unsafe archive path",
                        None,
                        name,
                    )
                )
            mode = (info.external_attr >> 16) & 0o170000
            if stat.S_ISLNK(mode):
                findings.append(
                    _finding(
                        "dependency_wheel_symlink",
                        "error",
                        "wheel contains a symbolic link entry",
                        None,
                        name,
                    )
                )
            if name.lower().endswith(_NATIVE_SUFFIXES):
                findings.append(
                    _finding(
                        "native_extension_entry",
                        "error",
                        "wheel contains a native extension",
                        None,
                        name,
                    )
                )

        unsafe_archive_codes = {
            "dependency_wheel_entry_limit",
            "dependency_wheel_duplicate_path",
            "dependency_wheel_expanded_limit",
            "dependency_wheel_entry_too_large",
            "dependency_wheel_encrypted_entry",
            "dependency_wheel_unsafe_path",
            "dependency_wheel_symlink",
        }
        if any(item["code"] in unsafe_archive_codes for item in findings):
            return (
                {
                    "path": path,
                    "filename": filename,
                    "sha256": digest,
                    "distribution": None,
                    "version": None,
                    "modules": [],
                    "pure_python": False,
                    "record_verified": False,
                    "entries": len(infos),
                },
                findings,
            )

        metadata_names = sorted(name for name in names if name.endswith(".dist-info/METADATA"))
        record_names = sorted(name for name in names if name.endswith(".dist-info/RECORD"))
        distribution: str | None = None
        version: str | None = None
        modules = _wheel_modules(archive, names)
        if len(metadata_names) != 1:
            findings.append(
                _finding(
                    "dependency_wheel_metadata",
                    "error",
                    "wheel must contain exactly one dist-info/METADATA file",
                    None,
                    filename,
                )
            )
        else:
            try:
                if info_by_name[metadata_names[0]].file_size > MAX_METADATA_BYTES:
                    raise ValueError("metadata too large")
                raw = archive.read(metadata_names[0])
                if len(raw) > MAX_METADATA_BYTES:
                    raise ValueError("metadata too large")
                parsed = Parser().parsestr(raw.decode("utf-8"))
                distribution = parsed.get("Name")
                version = parsed.get("Version")
                if not distribution or not version:
                    raise ValueError("missing Name or Version")
            except (KeyError, UnicodeDecodeError, ValueError):
                findings.append(
                    _finding(
                        "dependency_wheel_metadata",
                        "error",
                        "wheel METADATA must contain UTF-8 Name and Version fields",
                        None,
                        filename,
                    )
                )

        record_verified = False
        if len(record_names) != 1:
            findings.append(
                _finding(
                    "dependency_wheel_record",
                    "error",
                    "wheel must contain exactly one dist-info/RECORD file",
                    None,
                    filename,
                )
            )
        else:
            record_verified, record_findings = _verify_record(
                archive, names, record_names[0], filename
            )
            findings.extend(record_findings)

    return (
        {
            "path": path,
            "filename": filename,
            "sha256": digest,
            "distribution": distribution,
            "version": version,
            "modules": modules,
            "pure_python": pure_filename
            and not any(item["code"] == "native_extension_entry" for item in findings),
            "record_verified": record_verified,
            "entries": len(infos),
        },
        findings,
    )


def _wheel_modules(archive: zipfile.ZipFile, names: set[str]) -> list[str]:
    top_level_files = sorted(name for name in names if name.endswith(".dist-info/top_level.txt"))
    modules: set[str] = set()
    for name in top_level_files:
        try:
            if archive.getinfo(name).file_size > MAX_METADATA_BYTES:
                continue
            text = archive.read(name).decode("utf-8")
        except (KeyError, UnicodeDecodeError):
            continue
        modules.update(line.strip() for line in text.splitlines() if line.strip())
    for name in names:
        parts = PurePosixPath(name).parts
        if not parts:
            continue
        first = parts[0]
        if first.endswith((".dist-info", ".data")) or first.startswith("."):
            continue
        if len(parts) == 1 and first.endswith(".py"):
            modules.add(first[:-3])
        elif len(parts) > 1 and first.isidentifier():
            modules.add(first)
    return sorted(module for module in modules if module.isidentifier())


def _verify_record(
    archive: zipfile.ZipFile,
    names: set[str],
    record_name: str,
    filename: str,
) -> tuple[bool, list[ComponentizeFinding]]:
    findings: list[ComponentizeFinding] = []
    try:
        if archive.getinfo(record_name).file_size > MAX_METADATA_BYTES * 4:
            raise ValueError("record too large")
        raw = archive.read(record_name)
        if len(raw) > MAX_METADATA_BYTES * 4:
            raise ValueError("record too large")
        rows = list(csv.reader(io.StringIO(raw.decode("utf-8"))))
    except (KeyError, UnicodeDecodeError, csv.Error, ValueError):
        return False, [
            _finding(
                "dependency_wheel_record",
                "error",
                "wheel RECORD is malformed or oversized",
                None,
                filename,
            )
        ]

    recorded: set[str] = set()
    verified = True
    for row in rows:
        if len(row) != 3:
            verified = False
            continue
        name, encoded_hash, encoded_size = row
        recorded.add(name)
        if name not in names:
            verified = False
            continue
        if name == record_name and not encoded_hash and not encoded_size:
            continue
        if not encoded_hash.startswith("sha256=") or not encoded_size.isdigit():
            verified = False
            continue
        body = archive.read(name)
        expected = encoded_hash.removeprefix("sha256=")
        actual = base64.urlsafe_b64encode(hashlib.sha256(body).digest()).decode("ascii").rstrip("=")
        if actual != expected or len(body) != int(encoded_size):
            verified = False
    if names - recorded:
        verified = False
    if not verified:
        findings.append(
            _finding(
                "dependency_wheel_record_mismatch",
                "error",
                "wheel RECORD does not authenticate every archive entry",
                None,
                filename,
            )
        )
    return verified, findings


def _read_bounded(path: str, limit: int) -> bytes:
    with open(path, "rb") as handle:
        body = handle.read(limit + 1)
    if len(body) > limit:
        raise ValueError("artifact exceeds limit")
    return body


def _finding(
    code: str,
    severity: Literal["error", "warning"],
    message: str,
    line: int | None,
    subject: str | None,
) -> ComponentizeFinding:
    return {
        "code": code,
        "severity": severity,
        "message": message,
        "line": line,
        "subject": subject,
    }


def _deduplicate(findings: list[ComponentizeFinding]) -> list[ComponentizeFinding]:
    unique: dict[tuple[object, ...], ComponentizeFinding] = {}
    for finding in findings:
        key = (
            finding["code"],
            finding["severity"],
            finding["line"],
            finding["subject"],
        )
        unique.setdefault(key, finding)
    return sorted(
        unique.values(),
        key=lambda item: (
            item["line"] is None,
            item["line"] or 0,
            item["severity"],
            item["code"],
            item["subject"] or "",
        ),
    )
