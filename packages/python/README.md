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
and generated CLI exposure. Dependency-free inference covers `Annotated`
descriptions and constraints, `TypedDict` required and optional keys,
dataclasses, literals, unions/nullability, fixed and variadic tuples, and typed
mapping values. Parameters annotated as Context are excluded from the public
input schema and injected by the runner. The one-shot compatibility runner
supplies request identity and explicit unavailable capability handles.
Persistent brokered workers provide live HTTP, secrets, namespaced state,
logging, metrics, progress, and cancellation handles under the provider
declaration, deployment policy, actor scopes, and host availability
intersection.

Broker capability calls are async so they do not block the provider event loop:

~~~python
response = await ctx.http.request("GET", "https://api.example.com/data")
secret = await ctx.secrets.get("example-key")
current = await ctx.state.get("counter")
await ctx.state.set("counter", (current or 0) + 1)
await ctx.log.emit("info", "updated", counter=current)
await ctx.metrics.increment("updates")
await ctx.progress.update(1, total=1, message="done")
~~~

HTTP request and response bodies are lossless bytes (`body_bytes`) with
base64-encoded transport. Broker policy denials remain typed
`CapabilityUnavailableError` failures instead of being reported as worker
crashes.

The experimental componentize preflight statically inspects source and explicit
wheel evidence without importing or executing provider code:

~~~python
from soma_provider import scan_componentize_compatibility

report = scan_componentize_compatibility(
    source,
    filename="provider.py",
    wheel_files=["dependency-1.0.0-py3-none-any.whl"],
)
~~~

It fails closed on native extensions, non-pure wheels, dynamic imports, process,
thread, socket, native-FFI, and other ambient-authority assumptions. A compatible
report means only that the provider is eligible for later isolated build and
Wasmtime validation. It does not transpile Python or claim runtime compatibility.

Rust's provider-core manifest and adapter validation remain authoritative. The
`tests/soma_runner_protocol.py` module is also internal: it implements the
bounded length-prefixed JSON codec, version negotiation, and feature intersection
used by persistent-runner contract fixtures. One-shot remains the default
runtime. `SOMA_PYTHON_RUNNER_MODE=persistent` activates supervised
installed-wheel workers for catalog and invocation. Set
`SOMA_PYTHON_EXECUTION_PROFILE=brokered` with explicit broker policy to activate
the fail-closed capability and containment boundary.

`pyproject.toml` defines the `soma-provider` 0.2.x maturin mixed package for
Python 3.11 and newer. The pure-Python facade remains usable without a native
extension; built wheels include the private `soma_provider._soma_native` abi3
module for provider-core manifest validation and an SDK/native version check.
`uv.lock` pins development/build resolution, and the package smoke test builds
and installs the wheel in isolation. The repository now enforces an independent
`soma-provider-v*` tag and version parity across Python and Cargo metadata;
trusted PyPI publication, signing, provenance, and release execution remain
separate milestones. Replacing a Python implementation with WASM should preserve
the provider contract rather
than attempt to transpile arbitrary Python code. The canonical delivery status
and remaining milestones live in
[`docs/specs/python-provider-platform.md`](../../docs/specs/python-provider-platform.md).
