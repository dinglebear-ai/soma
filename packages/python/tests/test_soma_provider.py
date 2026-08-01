import asyncio
import base64
import inspect
import subprocess
import sys
import textwrap
import unittest
from dataclasses import dataclass
from collections.abc import Mapping, Sequence
from typing import Annotated, Literal, NotRequired, TypedDict, get_origin
from unittest.mock import patch

import soma_provider
import soma_provider._runtime as provider_runtime

from soma_provider import (
    CapabilityUnavailableError,
    Context,
    MetadataError,
    __version__,
    json_schema,
    native_available,
    native_build,
    provider,
    tool,
)


class SchemaProfile(TypedDict):
    name: Annotated[str, "Display name", {"minLength": 1, "maxLength": 80}]
    age: NotRequired[int]


@dataclass
class SchemaJob:
    name: str
    retries: int = 0


class CatPayload(TypedDict):
    kind: Literal["cat"]
    meows: int


class DogPayload(TypedDict):
    kind: Literal["dog"]
    barks: int


PetPayload = Annotated[
    CatPayload | DogPayload,
    {"discriminator": {"propertyName": "kind"}},
]


class GeneratedModelTests(unittest.TestCase):
    def test_generated_models_preserve_schema_maps_and_composition(self):
        from soma_provider.models import JsonSchema, McpPrimitiveOverlay, McpToolOverlay

        self.assertIs(get_origin(JsonSchema), dict)
        self.assertIs(McpToolOverlay, McpPrimitiveOverlay)


class NativeFallbackTests(unittest.TestCase):
    def test_source_sdk_does_not_require_native_extension(self):
        # Asserting `native_available()` directly would only be testing whether
        # this checkout happens to have a compiled extension sitting in the
        # source tree - `just test-python-package` builds one there, so the
        # result flipped depending on what had been run before. Block the
        # extension in a child interpreter instead, so this asserts the
        # fallback path itself rather than the state of the working tree.
        script = textwrap.dedent(
            """
            import sys

            class _BlockNative:
                def find_module(self, name, path=None):
                    return self if name.endswith("soma_provider._soma_native") else None

                def find_spec(self, name, path=None, target=None):
                    if name == "soma_provider._soma_native":
                        raise ImportError("native extension blocked for this test")
                    return None

            sys.meta_path.insert(0, _BlockNative())

            import soma_provider

            assert not soma_provider.native_available(), "native must be unavailable"
            assert soma_provider.native_build() is None, "native_build must be None"

            # The SDK must still be fully usable without the extension.
            @soma_provider.tool(name="fallback-echo")
            def echo(message: str) -> str:
                return message

            assert echo("ok") == "ok"
            assert soma_provider.provider(name="p", kind="python")["name"] == "p"
            print("FALLBACK_OK")
            """
        )
        result = subprocess.run(
            [sys.executable, "-c", script],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            result.returncode,
            0,
            f"source-only import failed:\nstdout={result.stdout}\nstderr={result.stderr}",
        )
        self.assertIn("FALLBACK_OK", result.stdout)

    def test_native_build_agrees_with_native_available(self):
        # Whether the extension is present varies by checkout; what must always
        # hold is that the two accessors agree.
        if native_available():
            self.assertIsNotNone(native_build())
        else:
            self.assertIsNone(native_build())


class ToolDecoratorTests(unittest.TestCase):
    def test_public_version_is_stable(self):
        self.assertEqual(__version__, "0.2.0")

    def test_bare_decorator_preserves_function_identity_and_shape(self):
        def echo(message: str) -> str:
            """Echo a message."""
            return message

        decorated = tool(echo)

        self.assertIs(decorated, echo)
        self.assertEqual(inspect.signature(decorated), inspect.signature(echo))
        self.assertEqual(decorated.__annotations__, {"message": str, "return": str})
        self.assertEqual(decorated.__doc__, "Echo a message.")
        self.assertEqual(
            decorated.__soma_tool__,
            {"schema_version": 1, "spec": {}},
        )

    def test_metadata_decorator_preserves_async_function_and_normalizes_json(self):
        async def reflect(message: str) -> dict:
            return {"message": message}

        decorated = tool(
            name="reflect-message",
            description="Reflect one message.",
            input_schema={"type": "object", "required": ("message",)},
            cli={"aliases": ("reflect",)},
            meta={"owner": "platform"},
        )(reflect)

        self.assertIs(decorated, reflect)
        self.assertTrue(inspect.iscoroutinefunction(decorated))
        self.assertEqual(
            decorated.__soma_tool__["spec"],
            {
                "name": "reflect-message",
                "description": "Reflect one message.",
                "input_schema": {"type": "object", "required": ["message"]},
                "cli": {"aliases": ["reflect"]},
                "meta": {"owner": "platform"},
            },
        )

    def test_rejects_unknown_or_non_json_metadata(self):
        with self.assertRaisesRegex(MetadataError, "unsupported Soma tool metadata"):
            tool(unknown=True)

        with self.assertRaisesRegex(MetadataError, "JSON-compatible"):
            tool(meta={"bad": object()})


class ProviderMetadataTests(unittest.TestCase):
    def test_provider_builds_detached_json_mapping(self):
        tags = ["network"]
        metadata = provider(
            name="network-tools",
            kind="python",
            capabilities=tags,
            meta={"owner": "platform"},
        )
        tags.append("mutated")

        self.assertEqual(
            metadata,
            {
                "name": "network-tools",
                "kind": "python",
                "capabilities": ["network"],
                "meta": {"owner": "platform"},
            },
        )

    def test_provider_rejects_unknown_fields(self):
        with self.assertRaisesRegex(MetadataError, "unsupported Soma provider metadata"):
            provider(name="bad", unknown=True)


class SchemaTests(unittest.TestCase):
    def test_dependency_free_schema_helpers(self):
        self.assertEqual(json_schema(str), {"type": "string"})
        self.assertEqual(
            json_schema(list[int]),
            {"type": "array", "items": {"type": "integer"}},
        )
        self.assertEqual(
            json_schema(str | None),
            {"type": ["string", "null"]},
        )
        self.assertEqual(
            provider_runtime.annotation_schema(str | None),
            {"anyOf": [{"type": "string"}, {"type": "null"}]},
        )
        self.assertEqual(
            provider_runtime.annotation_schema(str | None),
            {"anyOf": [{"type": "string"}, {"type": "null"}]},
        )
        self.assertEqual(
            json_schema(Sequence[int]),
            {"type": "array", "items": {"type": "integer"}},
        )
        self.assertEqual(
            json_schema(Mapping[str, int]),
            {"type": "object", "additionalProperties": {"type": "integer"}},
        )
        self.assertEqual(
            json_schema(set[str]),
            {"type": "array", "items": {"type": "string"}, "uniqueItems": True},
        )

    def test_annotated_typed_dict_dataclass_and_literal_schemas(self):
        self.assertEqual(
            json_schema(SchemaProfile),
            {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Display name",
                        "minLength": 1,
                        "maxLength": 80,
                    },
                    "age": {"type": "integer"},
                },
                "additionalProperties": False,
                "required": ["name"],
            },
        )
        self.assertEqual(
            json_schema(SchemaJob),
            {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "retries": {"type": "integer"},
                },
                "additionalProperties": False,
                "required": ["name"],
            },
        )
        self.assertEqual(json_schema(Literal["fast", "safe"]), {"enum": ["fast", "safe"]})
        self.assertEqual(
            json_schema(dict[str, int]),
            {"type": "object", "additionalProperties": {"type": "integer"}},
        )

    def test_discriminated_union_and_output_schema_inference(self):
        self.assertEqual(
            json_schema(PetPayload),
            {
                "anyOf": [
                    {
                        "type": "object",
                        "properties": {
                            "kind": {"enum": ["cat"]},
                            "meows": {"type": "integer"},
                        },
                        "additionalProperties": False,
                        "required": ["kind", "meows"],
                    },
                    {
                        "type": "object",
                        "properties": {
                            "kind": {"enum": ["dog"]},
                            "barks": {"type": "integer"},
                        },
                        "additionalProperties": False,
                        "required": ["barks", "kind"],
                    },
                ],
                "discriminator": {"propertyName": "kind"},
            },
        )

        def create_job(name: str) -> SchemaJob:
            return SchemaJob(name=name)

        self.assertEqual(
            provider_runtime.tool_output_schema(create_job, "python"),
            json_schema(SchemaJob),
        )

    def test_annotated_rejects_unknown_schema_keywords(self):
        with self.assertRaisesRegex(MetadataError, "unsupported Annotated"):
            json_schema(Annotated[str, {"unknownConstraint": 1}])
        with self.assertRaisesRegex(MetadataError, "discriminator propertyName"):
            json_schema(Annotated[CatPayload | DogPayload, {"discriminator": {}}])

    def test_sync_python_call_does_not_create_an_event_loop(self):
        def echo(value: int) -> dict[str, int]:
            return {"value": value}

        with patch.object(
            provider_runtime.asyncio,
            "run",
            side_effect=AssertionError("sync tool created an event loop"),
        ):
            self.assertEqual(
                provider_runtime.call_python_sync(echo, {"value": 7}, {}),
                {"value": 7},
            )

    def test_context_has_no_public_schema(self):
        with self.assertRaisesRegex(MetadataError, "runner-injected"):
            json_schema(Context)


class ContextTests(unittest.TestCase):
    def test_runner_context_is_immutable_and_carries_request_identity(self):
        context = Context._from_payload(
            {
                "request_id": 7,
                "provider": "network-tools",
                "action": "status",
                "surface": "mcp",
                "snapshot_id": "snapshot-4",
                "actor": {"actor_id": "user-1", "scopes": ["status.read"]},
                "trace": {"traceparent": "00-test"},
                "deadline_unix_ms": 1234,
            }
        )

        self.assertEqual(context.request.request_id, "7")
        self.assertEqual(context.request.provider, "network-tools")
        self.assertEqual(context.request.action, "status")
        self.assertEqual(context.request.surface, "mcp")
        self.assertEqual(context.request.snapshot_id, "snapshot-4")
        self.assertEqual(context.request.actor["actor_id"], "user-1")
        self.assertFalse(context.cancelled)
        with self.assertRaisesRegex(CapabilityUnavailableError, "http"):
            context.http.request
        with self.assertRaises(AttributeError):
            context.cancelled = True


class ContextConvenienceTests(unittest.IsolatedAsyncioTestCase):
    async def test_typed_context_conveniences_use_the_broker(self):
        calls = []
        cancelled = False

        def caller(method, invocation_id, payload):
            calls.append((method, invocation_id, payload))
            if method == "host.http":
                return {
                    "status": 200,
                    "body_base64": base64.b64encode(b'{"ok":true}').decode("ascii"),
                }
            if method == "host.secret":
                return "secret-value"
            if method == "host.state.get":
                return None
            if method == "host.cancelled":
                return cancelled
            return None

        previous = soma_provider._set_host_caller(caller)
        try:
            context = Context._from_payload(
                {
                    "request_id": "request-9",
                    "invocation_id": "invocation-9",
                    "provider": "typed-tools",
                    "action": "run",
                    "surface": "mcp",
                    "snapshot_id": "snapshot-9",
                }
            )
            self.assertEqual(
                await context.http_json(
                    "POST", "https://example.com/v1", body={"value": 1}
                ),
                {"ok": True},
            )
            self.assertEqual(await context.secret("billing-key"), "secret-value")
            self.assertEqual(await context.state_get("missing", 42), 42)
            await context.state_set("value", {"count": 1})
            await context.info("ready", worker="one")
            await context.report_progress(1, total=2, message="half")
            await context.check_cancelled()
        finally:
            soma_provider._set_host_caller(previous)

        methods = [method for method, _, _ in calls]
        self.assertEqual(
            methods,
            [
                "host.http",
                "host.secret",
                "host.state.get",
                "host.state.put",
                "host.log",
                "host.progress",
                "host.cancelled",
            ],
        )
        self.assertEqual(calls[0][1], "invocation-9")
        self.assertEqual(calls[0][2]["request"]["headers"]["content-type"], "application/json")

    async def test_check_cancelled_raises(self):
        def caller(method, _invocation_id, _payload):
            return method == "host.cancelled"

        previous = soma_provider._set_host_caller(caller)
        try:
            context = Context._from_payload(
                {
                    "request_id": "request-cancel",
                    "provider": "typed-tools",
                    "action": "run",
                    "surface": "mcp",
                    "snapshot_id": "snapshot-cancel",
                }
            )
            with self.assertRaises(asyncio.CancelledError):
                await context.check_cancelled()
        finally:
            soma_provider._set_host_caller(previous)


if __name__ == "__main__":
    unittest.main()
