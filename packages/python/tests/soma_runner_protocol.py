"""Dependency-free codec for Soma's private persistent runner protocol."""

from __future__ import annotations

import json
import struct
from collections.abc import Mapping, Sequence
from typing import Any

PROTOCOL_MAJOR = 1
PROTOCOL_MINOR = 0
MAX_FRAME_BYTES = 8 * 1024 * 1024


class ProtocolError(ValueError):
    """A malformed or incompatible runner protocol message."""


def negotiate_version(
    host: Mapping[str, int], worker: Mapping[str, int]
) -> dict[str, int]:
    """Reject major mismatches and negotiate the lower supported minor."""

    host_major = _version_part(host, "major")
    worker_major = _version_part(worker, "major")
    if host_major != worker_major:
        raise ProtocolError(
            f"Python runner protocol major mismatch: host {host_major}, "
            f"worker {worker_major}"
        )
    return {
        "major": host_major,
        "minor": min(_version_part(host, "minor"), _version_part(worker, "minor")),
    }


def negotiate_features(host: Sequence[str], worker: Sequence[str]) -> list[str]:
    """Return the feature intersection in host preference order."""

    worker_features = set(worker)
    return [feature for feature in host if feature in worker_features]


def encode_frame(message: Any) -> bytes:
    """Encode one message as a big-endian u32 length and compact UTF-8 JSON."""

    try:
        payload = json.dumps(
            message,
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise ProtocolError(f"invalid Python runner JSON: {error}") from error

    if len(payload) > MAX_FRAME_BYTES:
        raise ProtocolError(
            f"Python runner frame payload is {len(payload)} bytes; "
            f"limit is {MAX_FRAME_BYTES}"
        )
    return struct.pack(">I", len(payload)) + payload


def decode_frame(frame: bytes | bytearray | memoryview) -> Any:
    """Decode exactly one complete control frame."""

    raw = bytes(frame)
    if len(raw) < 4:
        raise ProtocolError(
            f"Python runner frame header is incomplete: got {len(raw)} bytes; expected 4"
        )

    declared = struct.unpack(">I", raw[:4])[0]
    if declared > MAX_FRAME_BYTES:
        raise ProtocolError(
            f"Python runner frame payload is {declared} bytes; limit is {MAX_FRAME_BYTES}"
        )

    payload = raw[4:]
    if len(payload) != declared:
        raise ProtocolError(
            f"Python runner frame length mismatch: declared {declared} bytes; "
            f"got {len(payload)}"
        )

    try:
        return json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ProtocolError(f"invalid Python runner JSON: {error}") from error


def _version_part(version: Mapping[str, int], name: str) -> int:
    value = version.get(name)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0 or value > 65535:
        raise ProtocolError(f"protocol {name} must be an unsigned 16-bit integer")
    return value
