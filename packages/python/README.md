# Soma Python provider helper

Soma embeds this dependency-free module into the Python provider bridge. A
drop-in provider can import the decorator without installing a package. The same
module is packaged as the `soma-provider` distribution for IDEs, tests, and Python
projects that want an explicit dependency:

~~~bash
uv pip install ./packages/python
~~~

Provider files use the same import in both modes:

~~~python
from soma_provider import Context, provider, tool

PROVIDER = provider(name="example", kind="python")

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
def greet(name: str, ctx: Context) -> dict:
    """Return a greeting with runner request identity."""
    return {
        "message": f"Hello, {name}!",
        "surface": ctx.request.surface,
    }
~~~

The decorator returns the original function unchanged and records only
JSON-compatible metadata. Omitted fields keep the adapter's existing defaults:
function name and docstring discovery, annotation-based input-schema inference,
and generated CLI exposure. Parameters annotated as Context are excluded from
the public input schema and injected by the runner. The one-shot compatibility
runner supplies request identity and explicit unavailable capability handles;
HTTP, secrets, state, logging, metrics, and cancellation become live through the
persistent capability broker milestone.

Rust's provider-core manifest and adapter validation remain authoritative. The
`tests/soma_runner_protocol.py` module is also internal: it implements the
bounded length-prefixed JSON codec, version negotiation, and feature intersection
used by persistent-runner contract fixtures. One-shot remains the default
runtime. `SOMA_PYTHON_RUNNER_MODE=persistent` activates supervised installed-wheel
workers for catalog and invocation; active cancellation and brokered Context
services are not implemented yet.

`pyproject.toml` defines the `soma-provider` 0.2.x maturin mixed package for
Python 3.11 and newer. The pure-Python facade remains usable without a native
extension; built wheels include the private `soma_provider._soma_native` abi3
module for provider-core manifest validation and an SDK/native version check.
`uv.lock` pins development/build resolution, and the package smoke test builds
and installs the wheel in isolation. Publication to PyPI remains a separate
milestone. Replacing
a Python implementation with WASM should preserve the provider contract rather
than attempt to transpile arbitrary Python code. The canonical delivery status
and remaining milestones live in
[`docs/specs/python-provider-platform.md`](../../docs/specs/python-provider-platform.md).
