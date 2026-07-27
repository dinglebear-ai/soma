"""Dependency-free authoring helpers for Soma Python providers.

This module is embedded by the Rust Python adapter. Provider files can import it
without installing a wheel or configuring PYTHONPATH.
"""

from __future__ import annotations

import json

__all__ = ["tool"]

_TOOL_SPEC_FIELDS = frozenset(
    {
        "name",
        "description",
        "title",
        "input_schema",
        "output_schema",
        "scope",
        "destructive",
        "requires_admin",
        "cost",
        "env",
        "limits",
        "mcp",
        "rest",
        "cli",
        "palette",
        "ui",
        "examples",
        "meta",
    }
)


def _normalize_spec(spec):
    unexpected = sorted(set(spec) - _TOOL_SPEC_FIELDS)
    if unexpected:
        names = ", ".join(unexpected)
        raise TypeError(f"unsupported Soma tool metadata: {names}")
    try:
        return json.loads(json.dumps(spec, allow_nan=False))
    except (TypeError, ValueError) as error:
        raise TypeError(f"Soma tool metadata must be JSON-compatible: {error}") from error


def tool(_function=None, /, **spec):
    """Annotate a function with metadata for Soma's provider bridge.

    The original function is returned unchanged. When a field is omitted, the
    Rust-backed bridge keeps its existing inference/default behavior.
    """

    normalized = _normalize_spec(spec)

    def decorate(function):
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
