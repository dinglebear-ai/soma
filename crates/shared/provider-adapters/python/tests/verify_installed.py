"""Smoke-test the installed soma-provider wheel outside its source tree."""

from importlib.metadata import version
from importlib.util import find_spec

from soma_provider import Context, __version__, provider, tool


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
    assert find_spec("soma_runner_protocol") is None


if __name__ == "__main__":
    main()
