# Drop-In Providers

`soma` loads provider files from `./providers` by default. Override the
directory with `SOMA_PROVIDER_DIR` at runtime or with
`soma providers ... --dir <path>` for local checks.

## Supported Files

| Extension | Provider kind | What is loaded |
|---|---|---|
| `.json` | `static-rust`, `mcp`, `openapi` | Provider manifest JSON |
| `.ts` | `ai-sdk` | `export default { ... }` provider catalog metadata |
| `.wasm` | `wasm` | `soma.provider` custom section (or a `.wasm.json` sidecar manifest) |
| `.py` | `python`, `langchain`, `llamaindex` | `PROVIDER` dict plus tool functions |

Disabled manifests with `"enabled": false` under `provider` are visible in
inspection output and are not registered at runtime.

## Manifest Shape

Every provider declares:

- `schema_version`: numeric manifest contract version, currently `1`.
- `provider.name`: stable provider identifier shown in inspection output.
- `provider.kind`: one of `static-rust`, `ai-sdk`, `wasm`, `mcp`, `openapi`, `python`, `langchain`, or `llamaindex`.
- `tools[].name`: action name exposed through CLI, MCP, and HTTP.
- `tools[].input_schema`: JSON Schema object for action input.
- `tools[].output_schema`: optional JSON Schema for action output.

Set `provider.enabled` to `false` when you want a manifest checked and documented
without loading it at runtime.

## Two CLI Surfaces

`soma providers` has two distinct subcommand groups. They report on different
things and have different safety guarantees — pick the one that matches what
you're checking.

### Non-executing: inspect files on disk

```bash
soma providers list                       # list drop-in provider files
soma providers status                     # summarize loaded/disabled/invalid counts
soma providers lint                       # like status, but exits non-zero on any invalid file
soma providers lint --dir ./examples/providers --json
```

These parse manifests (JSON/TS/WASM sidecar) but never execute handler code,
call MCP, or fetch OpenAPI. Safe to run before the runtime touches any
provider — e.g. in CI, before committing a new provider example, or to sanity
check a directory you're about to point `SOMA_PROVIDER_DIR` at.

Each file is also checked against the same semantic manifest validation the
live registry runs (duplicate tool names, reserved CLI commands, schema
shape, capability declarations, ...) — not just "does it deserialize." A file
that parses but would be rejected at load time is reported `invalid`, and
`lint` fails on it.

**Python providers are never inspected this way.** Extracting a `.py`
provider's catalog requires importing (and thus executing) the module — there
is no metadata-only path for Python. Non-executing inspection reports `.py`
files as `skipped` rather than importing them. Use `soma providers validate`
or `soma providers inspect` (below) to check a Python provider; those load
the real registry and accept that the module runs.

### Executing: inspect the live, loaded registry

```bash
soma providers validate                   # validate the loaded registry's compiled schemas
soma providers inspect                    # full inspection: surfaces, capability posture, schemas
soma providers test ACTION --json '{...}' # dispatch one provider action through the registry
```

These build the real `ProviderRegistry` first, which means TS/WASM providers
are instantiated and (for `test`) handlers actually run.

## Runtime Loading

CLI commands refresh providers on startup:

```bash
soma my_provider_action --json '{"message":"hello"}'
```

MCP servers refresh file providers when clients list tools or read the tools
resource, so a newly dropped provider appears without rebuilding the binary.

HTTP dispatch uses the same registry:

```bash
curl -sS -X POST http://127.0.0.1:40060/v1/tools/my_provider_action \
  -H 'content-type: application/json' \
  -d '{"message":"hello"}'
```

## MCP Providers

`mcp` providers infer their transport from `meta.mcp`: `url` selects
Streamable HTTP and `stdio.command` selects stdio. Use `timeout_ms` to bound
upstream calls, and pin upstream tool mapping in each tool's `meta.mcp` block.

## OpenAPI Providers

`openapi` providers pin a base URL in `meta.openapi.base_url`; each tool supplies
a relative operation path in `tools[].meta.openapi.path` or `tools[].rest.path`.
Operation paths must stay relative to the pinned base URL. Declare allowed
network hosts in `capabilities.network.allowed_hosts` when network capability is
enabled.

## Examples

See `examples/providers/`.
