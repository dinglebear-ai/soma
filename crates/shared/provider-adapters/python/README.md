# Soma Python provider helper

Soma embeds this dependency-free module into the Python provider bridge. A
drop-in provider can import the decorator without installing a package. The same
module is packaged as the `soma-provider` distribution for IDEs, tests, and Python
projects that want an explicit dependency:

~~~bash
uv pip install ./crates/shared/provider-adapters/python
~~~

Provider files use the same import in both modes:

~~~python
from soma_provider import tool

PROVIDER = {"name": "example", "kind": "python"}

@tool(
    name="greet",
    title="Greet",
    input_schema={
        "type": "object",
        "additionalProperties": False,
        "properties": {"name": {"type": "string"}},
        "required": ["name"],
    },
)
def greet(name: str) -> dict:
    """Return a greeting."""
    return {"message": f"Hello, {name}!"}
~~~

The decorator returns the original function unchanged and records only
JSON-compatible metadata. Omitted fields keep the adapter's existing defaults:
function name and docstring discovery, annotation-based input-schema inference,
and generated CLI exposure.

Rust's provider-core manifest and adapter validation remain authoritative. The
adjacent `soma_runner_protocol.py` module is also internal: it implements the
bounded length-prefixed JSON codec, version negotiation, and feature intersection
used by persistent-runner contract fixtures. The current runtime still executes
through the one-shot bridge.

`pyproject.toml` defines the dependency-free `soma-provider` 0.1.x package for
Python 3.10 and newer. `uv.lock` pins the development/build resolution, and the
package smoke test builds a wheel, installs it into an isolated environment, and
verifies that only the public authoring helper ships. PyO3/maturin bindings,
persistent workers, and publication to PyPI remain separate milestones. Replacing
a Python implementation with WASM should preserve the provider contract rather
than attempt to transpile arbitrary Python code.
