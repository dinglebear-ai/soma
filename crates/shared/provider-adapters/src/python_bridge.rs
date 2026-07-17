pub(crate) const PYTHON_BRIDGE: &str = r#"
import asyncio
import contextlib
import dataclasses
import importlib.util
import inspect
import json
import os
import re
import sys
import types
import typing
from pathlib import Path

MISSING = object()


def load_module(path):
    path = Path(path).resolve()
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location("_soma_python_provider", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import provider file {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def restrict_environment(allowed):
    allowed = {str(key) for key in (allowed or [])}
    for key in list(os.environ):
        if key not in allowed:
            del os.environ[key]


def provider_config(module):
    value = getattr(module, "PROVIDER", None)
    if isinstance(value, dict):
        return dict(value)
    return {}


def slug(value):
    value = re.sub(r"[^a-zA-Z0-9]+", "-", value).strip("-").lower()
    return value or "python-provider"


def expand_tools(module):
    raw = getattr(module, "TOOLS", MISSING)
    if raw is MISSING:
        raw = getattr(module, "tools", MISSING)
    if raw is MISSING:
        return None
    expanded = []
    for item in raw:
        to_tool_list = getattr(item, "to_tool_list", None)
        if callable(to_tool_list):
            expanded.extend(to_tool_list())
        else:
            expanded.append(item)
    return expanded


def public_functions(module):
    functions = []
    for name, value in vars(module).items():
        if name.startswith("_"):
            continue
        if inspect.isfunction(value) and getattr(value, "__module__", None) == module.__name__:
            functions.append(value)
    return functions


def provider_tools(module):
    tools = expand_tools(module)
    if tools is None:
        return public_functions(module)
    return tools


def detect_kind(module, tools, config):
    kind = config.get("kind") or getattr(module, "PROVIDER_KIND", None)
    if kind:
        return kind
    for tool in tools:
        if inspect.isfunction(tool):
            return "python"
        metadata = getattr(tool, "metadata", None)
        if metadata is not None and (
            hasattr(metadata, "fn_schema") or hasattr(metadata, "get_parameters_dict")
        ):
            return "llamaindex"
        if hasattr(tool, "args_schema") or hasattr(tool, "args") or hasattr(tool, "invoke"):
            return "langchain"
    raise RuntimeError("Python provider must expose PROVIDER['kind'] or detectable tools")


def jsonable(value, strict=False):
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, list) or isinstance(value, tuple):
        return [jsonable(item, strict=strict) for item in value]
    if isinstance(value, dict):
        return {str(key): jsonable(item, strict=strict) for key, item in value.items()}
    if dataclasses.is_dataclass(value):
        return jsonable(dataclasses.asdict(value), strict=strict)
    model_dump = getattr(value, "model_dump", None)
    if callable(model_dump):
        return jsonable(model_dump(), strict=strict)
    dict_method = getattr(value, "dict", None)
    if callable(dict_method):
        try:
            return jsonable(dict_method(), strict=strict)
        except TypeError:
            pass
    if hasattr(value, "content"):
        return {"content": jsonable(getattr(value, "content"), strict=strict)}
    if strict:
        type_name = f"{type(value).__module__}.{type(value).__qualname__}"
        raise TypeError(
            f"python_provider_unserializable_output: {type_name} is not JSON-compatible"
        )
    return str(value)


def model_schema(value):
    if value is None:
        return None
    if isinstance(value, dict):
        return jsonable(value)
    for method_name in ("model_json_schema", "schema"):
        method = getattr(value, method_name, None)
        if callable(method):
            return jsonable(method())
    return None


def object_schema(schema):
    schema = schema or {}
    if schema.get("type") == "object":
        schema.setdefault("additionalProperties", False)
        return schema
    if "properties" in schema:
        schema["type"] = "object"
        schema.setdefault("additionalProperties", False)
        return schema
    return {"type": "object", "additionalProperties": False, "properties": {}}


def langchain_schema(tool):
    schema = model_schema(getattr(tool, "args_schema", None))
    if schema:
        return object_schema(schema)
    args = getattr(tool, "args", None)
    if isinstance(args, dict):
        return object_schema({"type": "object", "properties": jsonable(args)})
    return object_schema(None)


def llamaindex_schema(tool):
    metadata = getattr(tool, "metadata", None)
    schema = model_schema(getattr(metadata, "fn_schema", None))
    if schema:
        return object_schema(schema)
    get_parameters = getattr(metadata, "get_parameters_dict", None)
    if callable(get_parameters):
        return object_schema(jsonable(get_parameters()))
    return object_schema(None)


def annotation_schema(annotation):
    if annotation is inspect._empty:
        return {}
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

    try:
        origin = typing.get_origin(annotation)
        args = typing.get_args(annotation)
    except AttributeError as error:
        if "UnionType" not in str(error) or hasattr(types, "UnionType"):
            raise
        origin = getattr(annotation, "__origin__", None)
        args = getattr(annotation, "__args__", ())
    union_origins = [typing.Union]
    union_type = getattr(types, "UnionType", None)
    if union_type is not None:
        union_origins.append(union_type)
    if origin in union_origins:
        includes_none = any(item is type(None) for item in args)
        non_none = [item for item in args if item is not type(None)]
        if len(non_none) == 1:
            schema = annotation_schema(non_none[0])
            if includes_none:
                return {"anyOf": [schema, {"type": "null"}]}
            return schema
        variants = [annotation_schema(item) for item in non_none]
        variants = [variant for variant in variants if variant]
        if includes_none:
            variants.append({"type": "null"})
        return {"anyOf": variants} if variants else {}
    if origin in (list, tuple, set, frozenset):
        item_schema = annotation_schema(args[0]) if args else {}
        return {"type": "array", "items": item_schema}
    if origin is dict:
        return {"type": "object", "additionalProperties": True}

    mapping = {
        str: "string",
        int: "integer",
        float: "number",
        bool: "boolean",
        dict: "object",
        list: "array",
    }
    schema_type = mapping.get(annotation)
    return {"type": schema_type} if schema_type else {}


def function_schema(tool):
    hints = {}
    try:
        hints = typing.get_type_hints(tool)
    except Exception as error:
        if "UnionType" in str(error) and not hasattr(types, "UnionType"):
            hints = {}
        else:
            name = getattr(tool, "__name__", "<unknown>")
            raise RuntimeError(
                f"Python tool {name!r} annotation resolution failed: {error}"
            ) from error
    properties = {}
    required = []
    signature = inspect.signature(tool)
    for name, parameter in signature.parameters.items():
        if name in ("self", "cls"):
            continue
        if parameter.kind is inspect.Parameter.POSITIONAL_ONLY:
            tool_label = getattr(tool, "__name__", "<unknown>")
            raise RuntimeError(
                f"Python tool {tool_label!r} parameter {name!r} is positional-only; "
                "plain Python provider parameters must be callable by JSON object key"
            )
        if parameter.kind in (
            inspect.Parameter.VAR_POSITIONAL,
            inspect.Parameter.VAR_KEYWORD,
        ):
            continue
        annotation = hints.get(name, parameter.annotation)
        properties[name] = annotation_schema(annotation)
        if parameter.default is inspect._empty:
            required.append(name)
    schema = {
        "type": "object",
        "additionalProperties": False,
        "properties": properties,
    }
    if required:
        schema["required"] = required
    return schema


def tool_name(tool, kind):
    if kind == "llamaindex":
        metadata = getattr(tool, "metadata", None)
        value = getattr(metadata, "name", None)
        if value:
            return value
    return getattr(tool, "name", None) or getattr(tool, "__name__", None)


def tool_description(tool, kind):
    if kind == "llamaindex":
        metadata = getattr(tool, "metadata", None)
        value = getattr(metadata, "description", None)
        if value:
            return value
    return getattr(tool, "description", None) or inspect.getdoc(tool) or "Python provider tool."


def tool_schema(tool, kind):
    if kind == "python":
        return function_schema(tool)
    if kind == "llamaindex":
        return llamaindex_schema(tool)
    return langchain_schema(tool)


def catalog(path):
    module = load_module(path)
    config = provider_config(module)
    tools = provider_tools(module)
    kind = detect_kind(module, tools, config)
    if kind not in ("python", "langchain", "llamaindex"):
        raise RuntimeError(f"unsupported Python provider kind {kind!r}")
    provider = {
        "name": config.get("name") or slug(Path(path).stem),
        "kind": kind,
    }
    for key in ("title", "description", "homepage", "source", "version", "enabled"):
        if key in config:
            provider[key] = config[key]
    output = {
        "schema_version": 1,
        "provider": provider,
        "tools": [],
        "meta": config.get("meta") or {},
    }
    for key in ("env", "capabilities", "docs", "plugin", "ui"):
        if key in config:
            output[key] = config[key]
    for tool in tools:
        name = tool_name(tool, kind)
        if not name:
            raise RuntimeError("Python provider tool is missing a name")
        output["tools"].append({
            "name": name,
            "description": tool_description(tool, kind),
            "input_schema": tool_schema(tool, kind),
            "cli": {"enabled": True, "command": name},
            "meta": {"python": {"adapter": kind}},
        })
    return output


def resolve_tool(module, action):
    config = provider_config(module)
    tools = provider_tools(module)
    kind = detect_kind(module, tools, config)
    for tool in tools:
        if tool_name(tool, kind) == action:
            return kind, tool
    raise RuntimeError(f"unknown Python provider action {action!r}")


async def maybe_await(value):
    if inspect.isawaitable(value):
        return await value
    return value


async def call_langchain(tool, params):
    ainvoke = getattr(tool, "ainvoke", None)
    if callable(ainvoke):
        return await maybe_await(ainvoke(params))
    invoke = getattr(tool, "invoke", None)
    if callable(invoke):
        return await maybe_await(invoke(params))
    if callable(tool):
        return await maybe_await(tool(**params))
    raise RuntimeError("LangChain tool is not callable")


async def call_llamaindex(tool, params):
    acall = getattr(tool, "acall", None)
    if callable(acall):
        return await maybe_await(acall(**params))
    call = getattr(tool, "call", None)
    if callable(call):
        return await maybe_await(call(**params))
    if callable(tool):
        return await maybe_await(tool(**params))
    raise RuntimeError("LlamaIndex tool is not callable")


async def call_python(tool, params):
    if callable(tool):
        return await maybe_await(tool(**params))
    raise RuntimeError("Python tool is not callable")


async def execute(path, action, params):
    module = load_module(path)
    kind, tool = resolve_tool(module, action)
    if kind == "python":
        return await call_python(tool, params)
    if kind == "llamaindex":
        return await call_llamaindex(tool, params)
    return await call_langchain(tool, params)


async def main():
    payload = json.loads(sys.stdin.buffer.read().decode("utf-8") or "{}")
    mode = payload.get("mode")
    with contextlib.redirect_stdout(sys.stderr):
        if mode == "catalog":
            restrict_environment([])
            result = catalog(payload["path"])
        elif mode == "call":
            restrict_environment(payload.get("env_keys") or [])
            result = await execute(payload["path"], payload["action"], payload.get("params") or {})
        else:
            raise RuntimeError(f"unknown Python bridge mode {mode!r}")
    sys.stdout.write(json.dumps(jsonable(result, strict=mode == "call"), separators=(",", ":")))


asyncio.run(main())
"#;

#[cfg(test)]
#[path = "python_bridge_tests.rs"]
mod tests;
