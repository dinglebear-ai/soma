import inspect
import unittest

from soma_provider import (
    CapabilityUnavailableError,
    Context,
    MetadataError,
    __version__,
    json_schema,
    provider,
    tool,
)


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
            {"anyOf": [{"type": "string"}, {"type": "null"}]},
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

        self.assertEqual(context.request.request_id, 7)
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


if __name__ == "__main__":
    unittest.main()
