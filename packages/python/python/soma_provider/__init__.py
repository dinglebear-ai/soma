"""Dependency-free authoring helpers for Soma Python providers.

This module is embedded by the Rust Python adapter. Provider files can import it
without installing a wheel or configuring PYTHONPATH.
"""

from __future__ import annotations

import asyncio
import base64
import dataclasses
import json
import types
import typing
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Any, ParamSpec, Protocol, TypeVar, TypedDict, overload

from ._componentize import (
    ComponentizeFinding,
    ComponentizeReport,
    scan_componentize_compatibility,
)

__version__ = "0.2.0"

try:
    from . import _soma_native as _native
except ImportError:
    _native = None
else:
    if _native.sdk_version() != __version__:
        raise ImportError(
            "soma-provider Python/native version mismatch: "
            f"Python {__version__}, native {_native.sdk_version()}"
        )

__all__ = [
    "__version__",
    "CapabilityUnavailableError",
    "ComponentizeFinding",
    "ComponentizeReport",
    "Context",
    "Http",
    "HttpResponse",
    "Log",
    "MetadataError",
    "Metrics",
    "Progress",
    "Cancellation",
    "ProviderMetadata",
    "Request",
    "Secrets",
    "SomaProviderError",
    "State",
    "ToolMetadata",
    "json_schema",
    "native_available",
    "native_build",
    "provider",
    "scan_componentize_compatibility",
    "tool",
    "validate_manifest",
]

_P = ParamSpec("_P")
_R = TypeVar("_R")


class SomaProviderError(Exception):
    """Base exception for failures exposed by the public provider SDK."""


class MetadataError(SomaProviderError, TypeError):
    """Raised when provider or tool metadata violates the SDK contract."""


class CapabilityUnavailableError(SomaProviderError):
    """Raised when a brokered host capability is unavailable in this runtime."""


def native_available() -> bool:
    """Return whether the matching private Rust extension is installed."""

    return _native is not None


def native_build() -> dict[str, Any] | None:
    """Return native compatibility metadata without exposing the extension."""

    if _native is None:
        return None
    return {
        "sdk_version": _native.sdk_version(),
        "provider_schema_version": _native.provider_schema_version(),
    }


def validate_manifest(manifest: str | Mapping[str, Any]) -> dict[str, Any]:
    """Validate and normalize a provider manifest through provider-core."""

    if _native is None:
        raise CapabilityUnavailableError(
            "native provider validation requires the installed soma-provider wheel"
        )
    document = manifest if isinstance(manifest, str) else json.dumps(manifest, allow_nan=False)
    return typing.cast(dict[str, Any], json.loads(_native.validate_manifest_json(document)))


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
    capabilities: dict[str, Any]
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

    request_id: str
    provider: str
    action: str
    surface: str
    snapshot_id: str
    actor: Mapping[str, Any] | None = None
    trace: Mapping[str, Any] | None = None
    deadline_unix_ms: int | None = None


class HttpResponse(TypedDict, total=False):
    status: int
    body: str
    body_base64: str
    body_bytes: bytes


class Http(Protocol):
    async def request(
        self,
        method: str,
        url: str,
        *,
        headers: Mapping[str, str] | None = None,
        body: bytes | None = None,
    ) -> HttpResponse:
        pass


class Secrets(Protocol):
    async def get(self, name: str) -> str:
        pass


class State(Protocol):
    async def get(self, key: str) -> Any:
        pass

    async def set(self, key: str, value: Any) -> None:
        pass


class Log(Protocol):
    async def emit(self, level: str, message: str, **fields: Any) -> None:
        pass


class Metrics(Protocol):
    async def increment(self, name: str, value: int = 1, **labels: str) -> None:
        pass

    async def duration(self, name: str, seconds: float, **labels: str) -> None:
        pass


class Progress(Protocol):
    async def update(
        self, current: int, *, total: int | None = None, message: str | None = None
    ) -> None:
        pass


class Cancellation(Protocol):
    async def is_cancelled(self) -> bool:
        pass


_host_caller: Callable[[str, str, dict[str, Any]], Any] | None = None


def _set_host_caller(
    caller: Callable[[str, str, dict[str, Any]], Any] | None,
) -> Callable[[str, str, dict[str, Any]], Any] | None:
    global _host_caller
    previous = _host_caller
    _host_caller = caller
    return previous


class _BrokerCapability:
    __slots__ = ("_invocation_id",)

    def __init__(self, invocation_id: str) -> None:
        self._invocation_id = invocation_id

    def _call(self, method: str, **payload: Any) -> Any:
        if _host_caller is None:
            raise CapabilityUnavailableError(
                f"Soma capability {method!r} is unavailable in the active Python runner"
            )
        return _host_caller(method, self._invocation_id, payload)

    async def _call_async(self, method: str, **payload: Any) -> Any:
        # Persistent native workers offload the blocking framed channel. The
        # component adapter marks its in-process WIT caller as direct because
        # componentized Python has no thread authority and host imports are
        # synchronous by construction.
        if _host_caller is not None and getattr(_host_caller, "__soma_direct__", False):
            return self._call(method, **payload)
        return await asyncio.to_thread(self._call, method, **payload)


class _BrokerHttp(_BrokerCapability):
    async def request(
        self,
        method: str,
        url: str,
        *,
        headers: Mapping[str, str] | None = None,
        body: bytes | None = None,
    ) -> HttpResponse:
        result = await self._call_async(
            "host.http",
            request={
                "method": method,
                "url": url,
                "headers": dict(headers or {}),
                "body_base64": None
                if body is None
                else base64.b64encode(body).decode("ascii"),
            },
        )
        if isinstance(result, dict) and isinstance(result.get("body_base64"), str):
            result = dict(result)
            result["body_bytes"] = base64.b64decode(
                result["body_base64"], validate=True
            )
        return result


class _BrokerSecrets(_BrokerCapability):
    async def get(self, name: str) -> str:
        return str(await self._call_async("host.secret", name=name))


class _BrokerState(_BrokerCapability):
    async def get(self, key: str) -> Any:
        return await self._call_async("host.state.get", key=key)

    async def set(self, key: str, value: Any) -> None:
        await self._call_async("host.state.put", key=key, value=value)


class _BrokerLog(_BrokerCapability):
    async def emit(self, level: str, message: str, **fields: Any) -> None:
        await self._call_async("host.log", level=level, message=message, fields=fields)


class _BrokerMetrics(_BrokerCapability):
    async def increment(self, name: str, value: int = 1, **labels: str) -> None:
        await self._call_async(
            "host.metric", name=name, value=value, attributes={"kind": "counter", **labels}
        )

    async def duration(self, name: str, seconds: float, **labels: str) -> None:
        await self._call_async(
            "host.metric",
            name=name,
            value=seconds,
            attributes={"kind": "duration_seconds", **labels},
        )


class _BrokerProgress(_BrokerCapability):
    async def update(
        self, current: int, *, total: int | None = None, message: str | None = None
    ) -> None:
        await self._call_async(
            "host.progress", current=current, total=total, message=message
        )


class _BrokerCancellation(_BrokerCapability):
    async def is_cancelled(self) -> bool:
        return bool(await self._call_async("host.cancelled"))


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
    progress: Progress
    cancellation: Cancellation
    cancelled: bool = False

    @classmethod
    def _from_payload(cls, payload: Mapping[str, Any]) -> Context:
        request = Request(
            request_id=str(payload["request_id"]),
            provider=str(payload["provider"]),
            action=str(payload["action"]),
            surface=str(payload["surface"]),
            snapshot_id=str(payload["snapshot_id"]),
            actor=_optional_mapping(payload.get("actor")),
            trace=_optional_mapping(payload.get("trace")),
            deadline_unix_ms=_optional_int(payload.get("deadline_unix_ms")),
        )
        invocation_id = str(payload.get("invocation_id", payload["request_id"]))
        brokered = _host_caller is not None
        return cls(
            request=request,
            http=_BrokerHttp(invocation_id) if brokered else _UnavailableCapability("http"),
            secrets=_BrokerSecrets(invocation_id) if brokered else _UnavailableCapability("secrets"),
            state=_BrokerState(invocation_id) if brokered else _UnavailableCapability("state"),
            log=_BrokerLog(invocation_id) if brokered else _UnavailableCapability("log"),
            metrics=_BrokerMetrics(invocation_id) if brokered else _UnavailableCapability("metrics"),
            progress=_BrokerProgress(invocation_id) if brokered else _UnavailableCapability("progress"),
            cancellation=_BrokerCancellation(invocation_id)
            if brokered
            else _UnavailableCapability("cancellation"),
            cancelled=bool(payload.get("cancelled", False)),
        )

    async def http_json(
        self,
        method: str,
        url: str,
        *,
        headers: Mapping[str, str] | None = None,
        body: Any = None,
    ) -> Any:
        """Call the brokered HTTP capability with a JSON request/response body."""

        encoded = None
        request_headers = dict(headers or {})
        if body is not None:
            encoded = json.dumps(body, allow_nan=False, separators=(",", ":")).encode("utf-8")
            request_headers.setdefault("content-type", "application/json")
        response = await self.http.request(
            method, url, headers=request_headers, body=encoded
        )
        payload = response.get("body_bytes")
        if not isinstance(payload, (bytes, bytearray)):
            text = response.get("body")
            if not isinstance(text, str):
                raise SomaProviderError("brokered HTTP response has no JSON body")
            payload = text.encode("utf-8")
        try:
            return json.loads(payload)
        except (TypeError, ValueError, UnicodeDecodeError) as error:
            raise SomaProviderError(f"brokered HTTP response is not valid JSON: {error}") from error

    async def secret(self, name: str) -> str:
        """Read one declared secret through the host broker."""

        return await self.secrets.get(name)

    async def state_get(self, key: str, default: Any = None) -> Any:
        """Read one namespaced state value and normalize missing nulls."""

        value = await self.state.get(key)
        return default if value is None else value

    async def state_set(self, key: str, value: Any) -> None:
        """Persist one JSON-compatible namespaced state value."""

        await self.state.set(key, _normalize_json(value, "state value"))

    async def info(self, message: str, **fields: Any) -> None:
        """Emit a structured informational provider log."""

        await self.log.emit("info", message, **fields)

    async def report_progress(
        self, current: int, *, total: int | None = None, message: str | None = None
    ) -> None:
        """Report bounded progress through the invocation-owned channel."""

        await self.progress.update(current, total=total, message=message)

    async def check_cancelled(self) -> None:
        """Raise ``asyncio.CancelledError`` when the host cancelled the call."""

        if self.cancelled or await self.cancellation.is_cancelled():
            raise asyncio.CancelledError


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


_ANNOTATED_SCHEMA_KEYS = frozenset(
    {
        "default",
        "deprecated",
        "description",
        "discriminator",
        "examples",
        "exclusiveMaximum",
        "exclusiveMinimum",
        "format",
        "maxItems",
        "maxLength",
        "maximum",
        "minItems",
        "minLength",
        "minimum",
        "multipleOf",
        "pattern",
        "title",
        "uniqueItems",
    }
)


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

    if typing.is_typeddict(annotation):
        return _typed_dict_schema(annotation)
    if isinstance(annotation, type) and dataclasses.is_dataclass(annotation):
        return _dataclass_schema(annotation)

    origin = typing.get_origin(annotation)
    args = typing.get_args(annotation)
    if origin is typing.Annotated:
        schema = json_schema(args[0])
        for metadata in args[1:]:
            if isinstance(metadata, str):
                if metadata:
                    schema.setdefault("description", metadata)
                continue
            if isinstance(metadata, Mapping):
                overlay = dict(metadata)
                unsupported = sorted(set(overlay) - _ANNOTATED_SCHEMA_KEYS)
                if unsupported:
                    raise MetadataError(
                        "unsupported Annotated JSON Schema metadata: "
                        + ", ".join(unsupported)
                    )
                if "discriminator" in overlay:
                    overlay["discriminator"] = _validate_discriminator(
                        overlay["discriminator"]
                    )
                schema.update(
                    typing.cast(
                        dict[str, Any],
                        _normalize_json(overlay, "Annotated JSON Schema metadata"),
                    )
                )
        return schema

    required_wrappers = {
        wrapper
        for wrapper in (
            getattr(typing, "Required", None),
            getattr(typing, "NotRequired", None),
        )
        if wrapper is not None
    }
    if origin in required_wrappers:
        return json_schema(args[0]) if args else {}

    union_origins = [typing.Union]
    union_type = getattr(types, "UnionType", None)
    if union_type is not None:
        union_origins.append(union_type)
    if origin in union_origins:
        return _canonical_union_schema([json_schema(item) for item in args])
    if origin is typing.Literal:
        values = typing.cast(list[Any], _normalize_json(list(args), "Literal values"))
        return {"enum": values}
    if origin in (list, set, frozenset, Sequence, typing.Sequence):
        schema: dict[str, Any] = {
            "type": "array",
            "items": json_schema(args[0]) if args else {},
        }
        if origin in (set, frozenset):
            schema["uniqueItems"] = True
        return schema
    if origin is tuple:
        if len(args) == 2 and args[1] is Ellipsis:
            return {"type": "array", "items": json_schema(args[0])}
        if args:
            return {
                "type": "array",
                "prefixItems": [json_schema(item) for item in args],
                "minItems": len(args),
                "maxItems": len(args),
            }
        return {"type": "array", "items": {}}
    if origin in (dict, Mapping, typing.Mapping):
        values = json_schema(args[1]) if len(args) == 2 else True
        return {"type": "object", "additionalProperties": values}

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


def _canonical_union_schema(variants: list[dict[str, Any]]) -> dict[str, Any]:
    flattened: list[dict[str, Any]] = []
    for variant in variants:
        nested = variant.get("anyOf") if isinstance(variant, dict) else None
        if isinstance(nested, list) and set(variant) == {"anyOf"}:
            flattened.extend(item for item in nested if isinstance(item, dict))
        else:
            flattened.append(variant)

    unique: list[dict[str, Any]] = []
    seen: set[str] = set()
    for variant in flattened:
        key = json.dumps(variant, sort_keys=True, separators=(",", ":"))
        if key not in seen:
            seen.add(key)
            unique.append(variant)

    simple_types: list[str] = []
    for variant in unique:
        value = variant.get("type")
        if not isinstance(value, str) or set(variant) != {"type"}:
            break
        simple_types.append(value)
    else:
        ordered = sorted(set(simple_types), key=lambda item: (item == "null", item))
        return {"type": ordered[0] if len(ordered) == 1 else ordered}

    return unique[0] if len(unique) == 1 else {"anyOf": unique}


def _validate_discriminator(value: Any) -> dict[str, Any]:
    if not isinstance(value, Mapping):
        raise MetadataError("Annotated discriminator must be an object")
    discriminator = dict(value)
    if set(discriminator) - {"propertyName", "mapping"}:
        raise MetadataError("Annotated discriminator supports propertyName and mapping only")
    property_name = discriminator.get("propertyName")
    if not isinstance(property_name, str) or not property_name:
        raise MetadataError("Annotated discriminator propertyName must be a non-empty string")
    mapping = discriminator.get("mapping")
    if mapping is not None and (
        not isinstance(mapping, Mapping)
        or any(not isinstance(key, str) or not isinstance(item, str) for key, item in mapping.items())
    ):
        raise MetadataError("Annotated discriminator mapping must contain string keys and values")
    return typing.cast(dict[str, Any], _normalize_json(discriminator, "Annotated discriminator"))


def _type_hints(annotation: Any) -> dict[str, Any]:
    try:
        return typing.get_type_hints(annotation, include_extras=True)
    except Exception:
        return dict(getattr(annotation, "__annotations__", {}))


def _typed_dict_schema(annotation: Any) -> dict[str, Any]:
    hints = _type_hints(annotation)
    required = sorted(str(key) for key in getattr(annotation, "__required_keys__", ()))
    schema: dict[str, Any] = {
        "type": "object",
        "properties": {name: json_schema(value) for name, value in hints.items()},
        "additionalProperties": False,
    }
    if required:
        schema["required"] = required
    return schema


def _dataclass_schema(annotation: type[Any]) -> dict[str, Any]:
    hints = _type_hints(annotation)
    properties: dict[str, Any] = {}
    required: list[str] = []
    for field in dataclasses.fields(annotation):
        properties[field.name] = json_schema(hints.get(field.name, field.type))
        if (
            field.default is dataclasses.MISSING
            and field.default_factory is dataclasses.MISSING
        ):
            required.append(field.name)
    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required:
        schema["required"] = required
    return schema


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
