"""Persistent Soma Python provider worker.

The installed module uses reserved stdin/stdout pipes as its framed control
channel on every supported platform. Provider-level Python stdout is redirected
to stderr, keeping it out of the channel without an inheritable side descriptor.
Tests may supply ``SOMA_PYTHON_RUNNER_FD`` to exercise a socket transport.
"""

from __future__ import annotations

import asyncio
import contextlib
import importlib.util
import json
import os
import struct
import sys
from pathlib import Path
from typing import Any

from . import _runtime
_FEATURES = ["describe", "invoke", "health", "drain", "shutdown"]
_PROTOCOL = {"major": 1, "minor": 0}
MAX_FRAME_BYTES = 8 * 1024 * 1024


class ProtocolError(ValueError):
    """A malformed private runner message."""


def encode_frame(message: dict[str, Any]) -> bytes:
    try:
        payload = json.dumps(message, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise ProtocolError(f"invalid Python runner JSON: {error}") from error
    if len(payload) > MAX_FRAME_BYTES:
        raise ProtocolError("Python runner frame exceeds limit")
    return struct.pack(">I", len(payload)) + payload


def decode_frame(frame: bytes) -> dict[str, Any]:
    if len(frame) < 4:
        raise ProtocolError("incomplete Python runner frame")
    expected = struct.unpack(">I", frame[:4])[0]
    payload = frame[4:]
    if expected != len(payload) or expected > MAX_FRAME_BYTES:
        raise ProtocolError("invalid Python runner frame length")
    value = json.loads(
        payload.decode("utf-8"),
        parse_constant=lambda value: (_ for _ in ()).throw(
            ProtocolError(f"non-finite JSON value: {value}")
        ),
    )
    if not isinstance(value, dict):
        raise ProtocolError("control message must be an object")
    return value


class FramedChannel:
    def __init__(self, fd: int | None) -> None:
        if fd is None:
            self._reader = sys.stdin.buffer
            self._writer = sys.stdout.buffer
        else:
            os.set_inheritable(fd, False)
            self._reader = os.fdopen(os.dup(fd), "rb", buffering=0)
            self._writer = os.fdopen(fd, "wb", buffering=0)

    def read(self) -> dict[str, Any]:
        header = self._reader.read(4)
        if len(header) != 4:
            raise EOFError("control channel closed")
        length = int.from_bytes(header, "big")
        if length > MAX_FRAME_BYTES:
            raise ProtocolError("Python runner frame exceeds limit")
        payload = self._reader.read(length)
        if len(payload) != length:
            raise EOFError("truncated control frame")
        message = decode_frame(header + payload)
        if not isinstance(message, dict):
            raise ProtocolError("control message must be an object")
        return message

    def write(self, message: dict[str, Any]) -> None:
        self._writer.write(encode_frame(message))
        self._writer.flush()


def _error(request_id: int | None, code: str, message: str) -> dict[str, Any]:
    result: dict[str, Any] = {
        "type": "reply",
        "status": "error",
        "error": {"code": code, "phase": "protocol", "retryable": False, "public_message": message},
    }
    if request_id is not None:
        result["request_id"] = request_id
    return result


class Worker:
    def __init__(self, channel: FramedChannel) -> None:
        self.channel = channel
        self.ready = False
        self.draining = False
        self.generation_id = ""
        self.module: Any = None
        self.catalog: dict[str, Any] | None = None

    def hello(self) -> dict[str, Any]:
        return {
            "type": "hello",
            "protocol": _PROTOCOL,
            "sdk_version": __import__("soma_provider").__version__,
            "python": {"implementation": sys.implementation.name, "version": sys.version.split()[0]},
            "features": _FEATURES,
        }

    def initialize(self, message: dict[str, Any]) -> dict[str, Any]:
        protocol = message.get("protocol")
        if not isinstance(protocol, dict) or protocol.get("major") != _PROTOCOL["major"]:
            return _error(message.get("request_id"), "python_protocol_mismatch", "Python runner protocol major mismatch")
        self.ready = True
        self.generation_id = str(message.get("generation_id", ""))
        return {
            "type": "ready",
            "protocol": _PROTOCOL,
            "features": [
                item for item in message.get("features", []) if item in _FEATURES
            ],
            "generation_id": self.generation_id,
        }

    def describe(self, message: dict[str, Any]) -> dict[str, Any]:
        path = Path(str(message["path"])).resolve(strict=True)
        if path.suffix != ".py":
            return _error(message.get("request_id"), "python_import_failed", "Python provider source must be a .py file")
        with contextlib.redirect_stdout(sys.stderr):
            catalog = _runtime.catalog(path)
            module = _runtime.load_module(path)
        self.module = module
        self.catalog = catalog
        return {"type": "reply", "status": "ok", "request_id": message["request_id"], "result": catalog}

    async def invoke(self, message: dict[str, Any]) -> dict[str, Any]:
        if self.draining:
            return _error(message.get("request_id"), "python_worker_draining", "Python worker is draining")
        if self.module is None:
            return _error(message.get("request_id"), "python_protocol_mismatch", "Python provider was not described")
        invocation = message.get("invocation", {})
        action = invocation.get("action")
        try:
            kind, tool = _runtime.resolve_tool(self.module, action)
            arguments = dict(invocation.get("arguments", {}))
            with contextlib.redirect_stdout(sys.stderr):
                if kind == "python":
                    result = await _runtime.call_python(tool, arguments, invocation)
                elif kind == "llamaindex":
                    result = await _runtime.call_llamaindex(tool, arguments)
                else:
                    result = await _runtime.call_langchain(tool, arguments)
            result = _runtime.jsonable(result, strict=True)
            json.dumps(result, allow_nan=False)
        except Exception:
            return _error(message.get("request_id"), "python_worker_crashed", "Python provider invocation failed")
        return {"type": "reply", "status": "ok", "request_id": message["request_id"], "result": result}

    async def serve(self) -> int:
        self.channel.write(self.hello())
        while True:
            try:
                message = self.channel.read()
            except (EOFError, ProtocolError):
                return 1
            kind = message.get("type")
            request_id = message.get("request_id")
            try:
                if not self.ready:
                    if kind != "initialize":
                        self.channel.write(_error(request_id, "python_protocol_mismatch", "Python runner is not initialized"))
                    else:
                        self.channel.write(self.initialize(message))
                elif kind == "request" and message.get("method") == "describe":
                    self.channel.write(self.describe(message))
                elif kind == "request" and message.get("method") == "invoke":
                    invocation_id = str(message.get("invocation", {}).get("invocation_id", ""))
                    self.channel.write({
                        "type": "reply",
                        "status": "accepted",
                        "request_id": request_id,
                        "invocation_id": invocation_id,
                        "state": "accepted",
                    })
                    self.channel.write(await self.invoke(message))
                elif kind == "request" and message.get("method") == "health":
                    self.channel.write({
                        "type": "reply",
                        "status": "health",
                        "request_id": request_id,
                        "health": "ready",
                        "generation_id": self.generation_id,
                    })
                elif kind == "request" and message.get("method") == "drain":
                    self.draining = True
                    self.channel.write({"type": "reply", "status": "ok", "request_id": request_id, "result": None})
                elif kind == "request" and message.get("method") == "shutdown":
                    self.channel.write({"type": "reply", "status": "ok", "request_id": request_id, "result": None})
                    return 0
                else:
                    self.channel.write(_error(request_id, "python_protocol_mismatch", "Unsupported Python runner request"))
            except Exception:
                self.channel.write(_error(request_id, "python_worker_crashed", "Python runner failed"))


def main() -> int:
    raw = os.environ.get("SOMA_PYTHON_RUNNER_FD")
    descriptor = int(raw) if raw is not None else None
    return asyncio.run(Worker(FramedChannel(descriptor)).serve())


if __name__ == "__main__":
    raise SystemExit(main())
