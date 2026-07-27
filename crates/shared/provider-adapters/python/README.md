# Soma Python provider helper

Soma embeds this dependency-free module into the Python provider bridge. A
drop-in provider can import the decorator without installing a package:

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

Rust's provider-core manifest and adapter validation remain authoritative. This
helper is not a standalone distribution yet; packaging with pyproject/uv,
PyO3/maturin bindings, persistent workers, and a public package release are
separate milestones. Replacing a Python implementation with WASM should preserve
the provider contract rather than attempt to transpile arbitrary Python code.
