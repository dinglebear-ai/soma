"""Smoke-test the installed soma-provider wheel outside its source tree."""

from importlib.metadata import version
from importlib.util import find_spec

from soma_provider import (
    Context,
    __version__,
    native_available,
    native_build,
    provider,
    tool,
    validate_manifest,
)


def main() -> None:
    assert version("soma-provider") == __version__

    @tool(name="installed-echo", meta={"source": "wheel"})
    def echo(message: str) -> str:
        return message

    assert echo("ready") == "ready"
    assert echo.__soma_tool__ == {
        "schema_version": 1,
        "spec": {"name": "installed-echo", "meta": {"source": "wheel"}},
    }
    assert provider(name="installed", kind="python")["name"] == "installed"
    assert Context.__module__ == "soma_provider"
    assert native_available()
    assert native_build() == {
        "sdk_version": __version__,
        "provider_schema_version": 1,
    }
    catalog = validate_manifest(
        {
            "schema_version": 1,
            "provider": {"name": "native-smoke", "kind": "static-rust"},
            "tools": [
                {
                    "name": "native_echo",
                    "description": "Validate through provider-core.",
                    "input_schema": {
                        "type": "object",
                        "additionalProperties": False,
                        "properties": {},
                    },
                }
            ],
        }
    )
    assert catalog["provider"]["name"] == "native-smoke"
    try:
        validate_manifest("not JSON")
    except ValueError as error:
        assert "invalid provider JSON" in str(error)
    else:
        raise AssertionError("native validation accepted invalid JSON")
    assert find_spec("soma_provider._soma_native") is not None
    assert find_spec("soma_provider.runner") is not None
    assert find_spec("soma_runner_protocol") is None


if __name__ == "__main__":
    main()
