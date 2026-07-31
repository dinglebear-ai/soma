"""Fail-closed static preflight for the experimental componentize-py path."""

from __future__ import annotations

import ast
import os
import sys
from collections.abc import Iterable
from typing import Literal, TypedDict


class ComponentizeFinding(TypedDict):
    code: str
    severity: Literal["error", "warning"]
    message: str
    line: int | None
    subject: str | None


class ComponentizeReport(TypedDict):
    schema_version: int
    experimental: bool
    compatible: bool
    requires_build_validation: bool
    filename: str
    imports: list[str]
    external_imports: list[str]
    wheel_files: list[str]
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

_NATIVE_SUFFIXES = (".so", ".pyd", ".dylib")


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
    """Return an experimental compatibility report without importing provider code."""

    wheels = sorted({os.fspath(value) for value in wheel_files})
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
    if external and not wheels:
        findings.append(
            _finding(
                "dependency_wheels_unscanned",
                "error",
                "external imports require an explicit dependency wheel scan",
                None,
                ",".join(external),
            )
        )

    for wheel in wheels:
        findings.extend(_scan_wheel(wheel))
    if external and wheels and not any(item["severity"] == "error" for item in findings):
        findings.append(
            _finding(
                "dependency_mapping_unverified",
                "warning",
                "pure-Python wheels were supplied, but import-to-distribution coverage still requires build validation",
                None,
                ",".join(external),
            )
        )

    findings = _deduplicate(findings)
    compatible = not any(item["severity"] == "error" for item in findings)
    return {
        "schema_version": 1,
        "experimental": True,
        "compatible": compatible,
        "requires_build_validation": compatible,
        "filename": filename,
        "imports": sorted(visitor.imports),
        "external_imports": external,
        "wheel_files": wheels,
        "findings": findings,
    }


def _scan_wheel(path: str) -> list[ComponentizeFinding]:
    name = os.path.basename(path)
    lower = name.lower()
    if lower.endswith(_NATIVE_SUFFIXES):
        return [
            _finding(
                "native_extension_artifact",
                "error",
                "native extension artifacts are unsupported by componentize-py",
                None,
                name,
            )
        ]
    if not lower.endswith(".whl"):
        return [
            _finding(
                "unsupported_dependency_artifact",
                "error",
                "dependency evidence must be a wheel file",
                None,
                name,
            )
        ]
    parts = name[:-4].split("-")
    if len(parts) < 5:
        return [
            _finding(
                "invalid_wheel_filename",
                "error",
                "wheel filename does not contain Python, ABI, and platform tags",
                None,
                name,
            )
        ]
    abi_tag = parts[-2].lower()
    platform_tag = parts[-1].lower()
    if abi_tag != "none" or platform_tag != "any":
        return [
            _finding(
                "native_wheel_unsupported",
                "error",
                "only pure-Python *-none-any wheels are eligible for the experiment",
                None,
                name,
            )
        ]
    return []


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
