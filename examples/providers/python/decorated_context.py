"""Decorated provider with runner-injected request context."""

from soma_provider import Context, provider, tool

PROVIDER = provider(
    name="context-example",
    kind="python",
    title="Context example",
)


@tool(
    name="request-summary",
    description="Summarize the current Soma invocation.",
    output_schema={"type": "object"},
)
def request_summary(message: str, ctx: Context) -> dict:
    return {
        "message": message,
        "provider": ctx.request.provider,
        "action": ctx.request.action,
        "surface": ctx.request.surface,
        "snapshot_id": ctx.request.snapshot_id,
    }
