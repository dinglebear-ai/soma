"""Zero-dependency asynchronous provider."""

import asyncio

from soma_provider import provider, tool

PROVIDER = provider(name="async-example", kind="python")


@tool(description="Return after yielding to the Python event loop.")
async def delayed_echo(message: str) -> dict:
    await asyncio.sleep(0)
    return {"message": message}
