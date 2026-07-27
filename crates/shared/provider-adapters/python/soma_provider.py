"""Dependency-free authoring helpers for Soma Python providers.

This module is embedded by the Rust Python adapter. Provider files can import it
without installing a wheel or configuring PYTHONPATH.
"""

from __future__ import annotations

import json
import types
import typing
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import Any, ParamSpec, Protocol, TypeVar, TypedDict, overload

__version__ = "0.2.0"
__all__ = [
    "__version__",
    "CapabilityUnavailableError",
    "Context",
    "Http",
    "Log",
    "MetadataError",
    "Metrics",
    "ProviderMetadata",
    "Request",
    "Secrets",
    "SomaProviderError",
    "State",
    "ToolMetadata",
    "json_schema",
    "provider",
    "tool",
]

_P = ParamSpec("_P")
_R = TypeVar("_R")


class SomaProviderError(Exception):
    """Base exception for failures exposed by the public provider SDK."""


class MetadataError(SomaProviderError, TypeError):
    """Raised when provider or tool metadata violates the SDK contract."""


class CapabilityUnavailableError(SomaProviderError):
    """Raised when a brokered host capability is unavailable in this runtime."""


class ProviderMetadata(TypedDict, total=False):
    name: str
    kind: str
    title: str
    description: str
    homepage: str
    source: str
    version: str
    enabled: bool
    env: list[dict[str, Any]]
    capabilities: list[str]
    docs: dict[str, Any]
    plugin: dict[str, Any]
    ui: dict[str, Any]
    meta: dict[str, Any]


class ToolMetadata(TypedDict, total=False):
    name: str
    description: str
    title: str
    input_schema: dict[str, Any]
    output_schema: dict[str, Any]
    scope: str
    destructive: bool
    requires_admin: bool
    cost: dict[str, Any]
    env: list[dict[str, Any]]
    limits: dict[str, Any]
    mcp: dict[str, Any]
    rest: dict[str, Any]
    cli: dict[str, Any]
    palette: dict[str, Any]
    ui: dict[str, Any]
    examples: list[dict[str, Any]]
    meta: dict[str, Any]


@dataclass(frozen=True, slots=True)
class Request:
    """Immutable request identity supplied by the Soma runner."""

    request_id: int
    provider: str
    action: str
    surface: str
    snapshot_id: str
    actor: Mapping[str, Any] | None = None
    trace: Mapping[str, Any] | None = None
    deadline_unix_ms: int | None = None


class Http(Protocol):
    async def request(
        self,
        method: str,
        url: str,
        *,
        headers: Mapping[str, str] | None = None,
        body: bytes | None = None,
    ) -> Any: ...


class Secrets(Protocol):
    async def get(self, name: str) -> str: ...


class State(Protocol):
    async def get(self, key: str) -> Any: ...

    async def set(self, key: str, value: Any) -> None: ...


class Log(Protocol):
    def emit(self, level: str, message: str, **fields: Any) -> None: ...


class Metrics(Protocol):
    def increment(self, name: str, value: int = 1, **labels: str) -> None: ...

    def duration(self, name: str, seconds: float, **labels: str) -> None: ...


class _UnavailableCapability:
    __slots__ = ("_name",)

    def __init__(self, name: str) -> None:
        self._name = name

    def __getattr__(self, operation: str) -> Any:
        raise CapabilityUnavailableError(
            f"Soma capability {self._name!r} is unavailable for operation {operation!r} "
            "in the active Python runner"
        )


@dataclass(frozen=True, slots=True)
class Context:
    """Runner-injected invocation context.

    Provider authors annotate a parameter with Context. Soma omits that
    parameter from the public input schema and supplies it at invocation time.
    Capability objects are brokered by the host; the one-shot compatibility
    runner exposes explicit unavailable handles until the persistent broker is
    activated.
    """

    request: Request
    http: Http
    secrets: Secrets
    state: State
    log: Log
    metrics: Metrics
    cancelled: bool = False

    @classmethod
    def _from_payload(cls, payload: Mapping[str, Any]) -> Context:
        request = Request(
            request_id=int(payload["request_id"]),
            provider=str(payload["provider"]),
            action=str(payload["action"]),
            surface=str(payload["surface"]),
            snapshot_id=str(payload["snapshot_id"]),
            actor=_optional_mapping(payload.get("actor")),
            trace=_optional_mapping(payload.get("trace")),
            deadline_unix_ms=_optional_int(payload.get("deadline_unix_ms")),
        )
        return cls(
            request=request,
            http=_UnavailableCapability("http"),
            secrets=_UnavailableCapability("secrets"),
            state=_UnavailableCapability("state"),
            log=_UnavailableCapability("log"),
            metrics=_UnavailableCapability("metrics"),
            cancelled=bool(payload.get("cancelled", False)),
        )


def _optional_mapping(value: Any) -> Mapping[str, Any] | None:
    if value is None:
        return None
    if not isinstance(value, Mapping):
        raise MetadataError("runner context mapping must be an object")
    return dict(value)


def _optional_int(value: Any) -> int | None:
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int):
        raise MetadataError("runner context deadline must be an integer")
    return value


_PROVIDER_FIELDS = frozenset(ProviderMetadata.__annotations__)
_TOOL_SPEC_FIELDS = frozenset(ToolMetadata.__annotations__)


def _normalize_json(value: Any, label: str) -> Any:
    try:
        return json.loads(json.dumps(value, allow_nan=False))
    except (TypeError, ValueError) as error:
        raise MetadataError(f"{label} must be JSON-compatible: {error}") from error


def _normalize_spec(spec: dict[str, Any]) -> dict[str, Any]:
    unexpected = sorted(set(spec) - _TOOL_SPEC_FIELDS)
    if unexpected:
        names = ", ".join(unexpected)
        raise MetadataError(f"unsupported Soma tool metadata: {names}")
    return _normalize_json(spec, "Soma tool metadata")


def provider(**metadata: Any) -> ProviderMetadata:
    """Build a validated PROVIDER mapping for a one-file provider."""

    unexpected = sorted(set(metadata) - _PROVIDER_FIELDS)
    if unexpected:
        names = ", ".join(unexpected)
        raise MetadataError(f"unsupported Soma provider metadata: {names}")
    return typing.cast(ProviderMetadata, _normalize_json(metadata, "Soma provider metadata"))


def json_schema(annotation: Any) -> dict[str, Any]:
    """Return the dependency-free JSON Schema subset understood by Soma."""

    if annotation is Any or annotation is typing.Any:
        return {}
    if annotation is Context or _is_context_annotation(annotation):
        raise MetadataError("Context is runner-injected and has no public JSON schema")
    if isinstance(annotation, str):
        simple = {
            "str": "string",
            "int": "integer",
            "float": "number",
            "bool": "boolean",
            "dict": "object",
            "list": "array",
        }.get(annotation)
        return {"type": simple} if simple else {}

    model_schema = getattr(annotation, "model_json_schema", None)
    if callable(model_schema):
        return typing.cast(dict[str, Any], _normalize_json(model_schema(), "model JSON schema"))

    origin = typing.get_origin(annotation)
    args = typing.get_args(annotation)
    union_origins = [typing.Union]
    union_type = getattr(types, "UnionType", None)
    if union_type is not None:
        union_origins.append(union_type)
    if origin in union_origins:
        return {"anyOf": [json_schema(item) for item in args]}
    if origin in (list, tuple, set, frozenset):
        return {"type": "array", "items": json_schema(args[0]) if args else {}}
    if origin is dict:
        return {"type": "object", "additionalProperties": True}

    schema_type = {
        str: "string",
        int: "integer",
        float: "number",
        bool: "boolean",
        dict: "object",
        list: "array",
        type(None): "null",
    }.get(annotation)
    return {"type": schema_type} if schema_type else {}


def _is_context_annotation(annotation: Any) -> bool:
    if annotation is Context:
        return True
    if isinstance(annotation, str):
        return annotation in {"Context", "soma_provider.Context"}
    return (
        getattr(annotation, "__name__", None) == "Context"
        and getattr(annotation, "__module__", None) == "soma_provider"
    )


@overload
def tool(
    _function: Callable[_P, _R], /, **spec: Any
) -> Callable[_P, _R]: ...


@overload
def tool(
    _function: None = None, /, **spec: Any
) -> Callable[[Callable[_P, _R]], Callable[_P, _R]]: ...


def tool(_function=None, /, **spec: Any):
    """Annotate a function with metadata for Soma's provider bridge.

    The original function is returned unchanged. When a field is omitted, the
    Rust-backed bridge keeps its existing inference/default behavior.
    """

    normalized = _normalize_spec(spec)

    def decorate(function: Callable[_P, _R]) -> Callable[_P, _R]:
        if not callable(function):
            raise TypeError("@tool can only decorate a callable")
        function.__soma_tool__ = {
            "schema_version": 1,
            "spec": dict(normalized),
        }
        return function

    if _function is None:
        return decorate
    return decorate(_function)
