#!/usr/bin/env python3
"""Render dependency-free Python provider models and editor stubs."""

from __future__ import annotations

import json
import keyword
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "crates/shared/provider-core/provider-manifest.schema.json"
PACKAGE = ROOT / "packages/python/python/soma_provider"
MODELS = PACKAGE / "models.py"
MODELS_STUB = PACKAGE / "models.pyi"
PACKAGE_STUB = PACKAGE / "__init__.pyi"
COMPONENTIZE_STUB = PACKAGE / "_componentize.pyi"


def class_name(value: str) -> str:
    words = re.findall(r"[A-Z]+(?=[A-Z][a-z]|$)|[A-Z]?[a-z]+|\d+", value)
    return "".join(word[:1].upper() + word[1:] for word in words) or "Model"


def identifier(value: str) -> str:
    value = re.sub(r"\W", "_", value)
    if not value or value[0].isdigit():
        value = f"field_{value}"
    if keyword.iskeyword(value):
        value += "_"
    return value


def type_expr(schema: Any) -> str:
    if schema is True:
        return "Any"
    if schema is False or not isinstance(schema, dict):
        return "Never"
    if "$ref" in schema:
        return class_name(str(schema["$ref"]).rsplit("/", 1)[-1])
    all_of = schema.get("allOf")
    if isinstance(all_of, list):
        rendered = list(dict.fromkeys(type_expr(item) for item in all_of))
        if len(rendered) == 1:
            return rendered[0]
        return "Any"
    variants = schema.get("oneOf") or schema.get("anyOf")
    if isinstance(variants, list):
        rendered = list(dict.fromkeys(type_expr(item) for item in variants))
        return " | ".join(rendered) if rendered else "Any"
    enum = schema.get("enum")
    if isinstance(enum, list) and enum:
        return "Literal[" + ", ".join(repr(item) for item in enum) + "]"
    const = schema.get("const")
    if const is not None:
        return f"Literal[{const!r}]"
    schema_type = schema.get("type")
    if isinstance(schema_type, list):
        rendered = list(dict.fromkeys(type_expr({"type": item}) for item in schema_type))
        return " | ".join(rendered)
    if schema_type == "string":
        return "str"
    if schema_type == "integer":
        return "int"
    if schema_type == "number":
        return "float"
    if schema_type == "boolean":
        return "bool"
    if schema_type == "null":
        return "None"
    if schema_type == "array":
        return f"list[{type_expr(schema.get('items', {}))}]"
    if schema_type == "object" or "properties" in schema:
        additional = schema.get("additionalProperties")
        if additional not in (None, False):
            return f"dict[str, {type_expr(additional)}]"
        return "dict[str, Any]"
    return "Any"


def render_models() -> str:
    document = json.loads(SCHEMA.read_text(encoding="utf-8"))
    definitions: dict[str, dict[str, Any]] = document.get("$defs", {})
    names = [class_name(name) for name in sorted(definitions)]
    lines = [
        '"""Generated typed provider catalog models. Do not edit by hand."""',
        "",
        "from __future__ import annotations",
        "",
        "from typing import __GENERATED_IMPORTS__",
        "",
        "__all__ = [",
        *[f'    "{name}",' for name in names],
        "]",
        "",
    ]
    for key in sorted(definitions):
        name = class_name(key)
        schema = definitions[key]
        properties = schema.get("properties")
        if isinstance(properties, dict) and properties:
            required = set(schema.get("required", []))
            lines.append(f"class {name}(TypedDict, total=False):")
            description = str(schema.get("description", "")).strip()
            if description:
                lines.append(f"    {description!r}")
            for field, field_schema in sorted(properties.items()):
                wrapper = "Required" if field in required else "NotRequired"
                lines.append(
                    f"    {identifier(field)}: {wrapper}[{type_expr(field_schema)}]"
                )
            lines.append("")
        elif schema.get("type") == "object" and schema.get("additionalProperties") is False:
            lines.append(f"class {name}(TypedDict, total=False):")
            lines.append("    pass")
            lines.append("")
        else:
            lines.append(f"{name} = {type_expr(schema)}")
            lines.append("")
    typing_imports = ["Any", "Literal", "NotRequired", "Required", "TypedDict"]
    if any("Never" in line for line in lines):
        typing_imports.insert(2, "Never")
    lines[4] = f"from typing import {', '.join(typing_imports)}"
    return chr(10).join(lines).rstrip() + chr(10)


def render_package_stub() -> str:
    return '''"""Typing surface for soma-provider."""
from collections.abc import Callable, Mapping
from typing import Any, Protocol, TypeVar, overload
from . import models as models
from ._componentize import ComponentizeFinding, ComponentizeReport, ComponentizeWheelEvidence, scan_componentize_compatibility

__version__: str
F = TypeVar("F", bound=Callable[..., Any])

class SomaProviderError(Exception): ...
class SchemaError(SomaProviderError): ...
class MetadataError(SomaProviderError): ...
class CapabilityUnavailableError(SomaProviderError): ...
class HttpResponse(dict[str, Any]): ...
class Request: ...
class Context:
    request: Request
    cancelled: bool
    async def http_json(self, method: str, url: str, *, headers: Mapping[str, str] | None = ..., body: Any = ...) -> Any: ...
    async def secret(self, name: str) -> str: ...
    async def state_get(self, key: str, default: Any = ...) -> Any: ...
    async def state_set(self, key: str, value: Any) -> None: ...
    async def info(self, message: str, **fields: Any) -> None: ...
    async def report_progress(self, current: int, *, total: int | None = ..., message: str | None = ...) -> None: ...
    async def check_cancelled(self) -> None: ...

@overload
def tool(function: F, /) -> F: ...
@overload
def tool(**metadata: Any) -> Callable[[F], F]: ...
def provider(**metadata: Any) -> dict[str, Any]: ...
def json_schema(annotation: Any) -> dict[str, Any]: ...
'''


def render_componentize_stub() -> str:
    return '''from collections.abc import Iterable
from typing import Literal, TypedDict

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
def scan_componentize_compatibility(source: str, *, filename: str = ..., wheel_files: Iterable[str] = ...) -> ComponentizeReport: ...
'''


def generated_files() -> dict[Path, str]:
    models = render_models()
    return {
        MODELS: models,
        MODELS_STUB: models,
        PACKAGE_STUB: render_package_stub(),
        COMPONENTIZE_STUB: render_componentize_stub(),
    }
