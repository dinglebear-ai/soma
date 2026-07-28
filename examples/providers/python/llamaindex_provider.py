# /// script
# requires-python = ">=3.11"
# dependencies = ["llama-index-core>=0.12,<1"]
# ///
"""LlamaIndex tool exposed through Soma's compatibility adapter."""

from llama_index.core.tools import FunctionTool

PROVIDER = {"name": "llamaindex-example", "kind": "llamaindex"}


def lookup(query: str) -> dict:
    """Return a demonstration lookup result."""
    return {"query": query, "result": "demo"}


TOOLS = [FunctionTool.from_defaults(fn=lookup)]
