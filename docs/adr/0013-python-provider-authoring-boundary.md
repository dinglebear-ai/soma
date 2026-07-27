# ADR 0013: Keep Python provider authoring embedded and contract-first

## Status

Accepted

## Context

Soma already supports plain Python, LangChain, and LlamaIndex provider files by
importing them in a bounded one-shot Python sidecar. Plain functions are easy to
start with, but authors need an explicit way to set the complete provider tool
metadata without duplicating Rust manifest construction or adopting a framework.

Python is also useful as an incubation path for provider behavior that may later
need stronger portability, startup, or isolation properties. That path needs a
stable boundary; arbitrary Python source cannot be mechanically converted into a
reliable WebAssembly component.

## Decision

The canonical provider contract remains the Rust `provider-core`
`ProviderManifest`/`ToolSpec` model and its JSON Schema validation. Python
metadata is authoring input to that contract, not a second provider model.

The initial authoring helper is a dependency-free module at
`crates/shared/provider-adapters/python/soma_provider.py`. The Rust adapter embeds
it into the Python bridge and registers it as `sys.modules["soma_provider"]`
before importing a provider. A drop-in file can therefore use
`from soma_provider import tool` without a wheel, `PYTHONPATH`, or inherited
process environment.

`@tool` returns the original function unchanged and attaches versioned,
JSON-compatible metadata. For plain Python tools, resolution order is:

1. explicit decorator metadata;
2. existing function name, docstring, and annotation inference;
3. adapter defaults.

Explicit input schemas skip annotation resolution, but callable-shape checks
still reject positional-only parameters because execution dispatches JSON object
keys as keyword arguments. Decorated names are used consistently by catalog and
call dispatch. CLI metadata shallowly overlays the generated
`{"enabled": true, "command": <resolved name>}` value. User metadata is
preserved, while `meta.python.adapter` is always set by the bridge and cannot be
spoofed.

Rust and Python communicate with a private, schema-versioned NDJSON envelope.
Each one-shot catalog or call request carries a request ID, and the response must
match its version, ID, and mode. Transport capture has bounded envelope headroom;
the extracted catalog or call payload is still checked against the original
payload budget. Catalog JSON remains raw until the canonical Rust manifest
validator accepts it.

Existing raw `PROVIDER` dictionaries, public-function discovery, explicit
`TOOLS = []`, LangChain tools, and LlamaIndex tools remain supported.

A Python provider graduates to WebAssembly by reimplementing its behavior behind
the same provider manifest, schemas, action names, and surface overlays. The
provider contract is portable; arbitrary Python source is not treated as a WASM
transpilation input.

## Non-goals

- No standalone PyPI package or public compatibility promise in this slice.
- No `pyproject.toml`, uv lockfile, maturin, or PyO3 binding layer yet.
- No persistent Python worker pool, handshake negotiation, or cancellation
  protocol yet.
- No automatic translation of Python, LangChain, or LlamaIndex code into WASM.
- No change to the provider manifest schema or drop-in directory layout.

## Security Notes

Python provider files are trusted executable code. Clearing the child environment
and enforcing time/input/output bounds limits accidental exposure and resource
use, but it is not an operating-system sandbox: imported code still has the
filesystem, network, and process authority of the Soma service account. Catalog
refresh executes module top-level code with no provider secrets; call execution
receives only declared provider/tool environment values.

The worker protocol is private and defensive, not a trust boundary. Rust remains
responsible for manifest validation, input/output schema checks, authorization,
capability policy, and public error shaping.

The current snapshot ID binds dispatch to catalog metadata, not to Python source
bytes or imported dependency bytes. The one-shot worker reloads the provider at
call time, so a trusted local edit between cataloging and dispatch can run under
the prior metadata until refresh. Source hashing, dependency pinning, or explicit
drift rejection is deferred to a later hardening phase.

## Consequences

Authors gain an explicit, zero-install decorator while simple legacy providers
continue to work unchanged. The helper ships with Soma as part of the adapter,
so its protocol and metadata version move with the host until a separate package
and compatibility matrix are justified.

Future Python bindings can expose more Rust-backed helpers without changing the
manifest contract. Future WASM providers can preserve the same catalog and
surface behavior while replacing only the implementation runtime.
