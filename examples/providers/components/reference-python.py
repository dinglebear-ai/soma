from soma_provider import provider, tool

PROVIDER = provider(name="reference-conformance", kind="python")


@tool
def conformance_echo(value: object) -> dict[str, object]:
    return {"ok": True, "echo": value}
