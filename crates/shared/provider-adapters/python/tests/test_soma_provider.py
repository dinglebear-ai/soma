import inspect
import unittest

from soma_provider import __version__, tool


class ToolDecoratorTests(unittest.TestCase):
    def test_public_version_is_stable(self):
        self.assertEqual(__version__, "0.1.0")

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
        with self.assertRaisesRegex(TypeError, "unsupported Soma tool metadata"):
            tool(unknown=True)

        with self.assertRaisesRegex(TypeError, "JSON-compatible"):
            tool(meta={"bad": object()})


if __name__ == "__main__":
    unittest.main()
