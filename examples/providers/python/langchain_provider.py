# /// script
# requires-python = ">=3.11"
# dependencies = ["langchain-core>=0.3,<2"]
# ///
"""LangChain tool exposed through Soma's compatibility adapter."""

from langchain_core.tools import tool

PROVIDER = {"name": "langchain-example", "kind": "langchain"}


@tool
def word_count(text: str) -> int:
    """Count whitespace-separated words."""
    return len(text.split())


TOOLS = [word_count]
