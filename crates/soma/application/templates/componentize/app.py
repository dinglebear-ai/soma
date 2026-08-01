"""Generated Soma componentize-py adapter. Do not edit in the workspace."""

import json

import provider_impl
import soma_provider
import soma_provider._runtime as runtime
from componentize_py_types import Err
from soma_wit import SomaWit as WitWorldProtocol
from soma_wit.imports import host


def _json(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":"))


def _host_call(method, _invocation_id, payload):
    if method == "host.http":
        return json.loads(host.http(_json(payload["request"])))
    if method == "host.secret":
        return host.secret(str(payload["name"]))
    if method == "host.state.get":
        return json.loads(host.state_get(str(payload["key"])))
    if method == "host.state.put":
        host.state_put(str(payload["key"]), _json(payload.get("value")))
        return None
    if method == "host.log":
        fields = {key: value for key, value in payload.items() if key not in {"level", "message"}}
        host.log(str(payload["level"]), str(payload["message"]), _json(fields))
        return None
    if method == "host.metric":
        host.metric(
            str(payload["name"]),
            float(payload["value"]),
            _json(payload.get("attributes", {})),
        )
        return None
    if method == "host.progress":
        host.progress(
            int(payload["current"]),
            int(payload["total"]) if payload.get("total") is not None else None,
            str(payload["message"]) if payload.get("message") is not None else None,
        )
        return None
    if method == "host.cancelled":
        return False
    raise RuntimeError(f"unsupported Soma component host call {method!r}")


_host_call.__soma_direct__ = True


class SomaWit(WitWorldProtocol):
    def invoke(self, input_json: str) -> str:
        try:
            envelope = json.loads(input_json)
            if not isinstance(envelope, dict):
                raise ValueError("component invocation envelope must be an object")
            action = envelope.get("action")
            arguments = envelope.get("arguments", {})
            if not isinstance(action, str) or not action:
                raise ValueError("component invocation requires an action")
            if not isinstance(arguments, dict):
                raise ValueError("component invocation arguments must be an object")
            kind, tool = runtime.resolve_tool(provider_impl, action)
            if kind != "python":
                raise ValueError("componentize-py supports plain Python Soma tools only")
            previous = soma_provider._set_host_caller(_host_call)
            try:
                result = runtime.call_python_sync(tool, arguments, envelope)
            finally:
                soma_provider._set_host_caller(previous)
            return _json(runtime.jsonable(result, strict=True))
        except Err:
            raise
        except Exception as error:
            raise Err(str(error)) from error
