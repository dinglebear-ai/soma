# Drop-In Providers

`soma` loads provider files from `./providers` by default. Override the
directory with `SOMA_PROVIDER_DIR` at runtime or with
`soma providers ... --dir <path>` for local checks.

## Structured Directory Layout

Beyond root-level files, `soma` also scans a structured layout:

```text
providers/
  tools/       # .json, .ts, .wasm, .py — same rules as root
  prompts/     # .md — same rules as root
  resources/   # any file (recursive) — see "Resources" below
```

`tools/` and `prompts/` are flat (non-recursive) and use the exact same
file-type rules as root-level files — they're purely an organizational
convenience. Root-level files keep working unchanged; new examples and docs
prefer the structured layout. See
`docs/contracts/drop-in-provider-layout.md` for the full contract.

## Supported Files

| Extension | Provider kind | What is loaded |
|---|---|---|
| `.json` | `static-rust`, `mcp`, `openapi` | Provider manifest JSON |
| `.ts` | `ai-sdk` | `export default { ... }` provider catalog metadata |
| `.wasm` | `wasm` | `soma.provider` custom section (or a `.wasm.json` sidecar manifest) |
| `.py` | `python`, `langchain`, `llamaindex` | `PROVIDER` dict plus tool functions |
| `.md` | `static-rust` prompt | Markdown prompt exposed through MCP prompts |

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
- `prompts[].template`: prompt body returned by MCP `prompts/get`.

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

These parse manifests (JSON/TS/WASM sidecar/Markdown) but never execute handler
code, call MCP, or fetch OpenAPI. Safe to run before the runtime touches any
provider — e.g. in CI, before committing a new provider example, or to sanity
check a directory you're about to point `SOMA_PROVIDER_DIR` at.

Each file is checked against the same semantic manifest validation the live
registry runs (duplicate tool names within a file, reserved CLI commands,
schema shape, capability declarations, ...) — not just "does it deserialize."
On top of that, files are also checked *against each other* and *against the
built-in `static-rust` provider* every `soma` binary loads alongside drop-in
files: two files (or a file and a built-in action, e.g. `status`) can each be
individually valid and still collide once loaded together (same provider
name, same action/tool name, same REST route, same CLI command/alias, same
MCP primitive name) — the live registry rejects that combination too. Either
kind of failure is reported `invalid`, and `lint` fails on it.

A REST route can also be unreachable for a reason the provider registry
itself doesn't check: `apps/soma/src/routes.rs` wires `/v1/capabilities`,
`/v1/providers`, `/v1/greet`, `/v1/echo`, `/v1/status`, `/v1/help`, and
`/v1/tools/{action}` directly on the same router, ahead of the dynamic
`/v1/{*path}` fallback that dispatches to provider-declared routes. Axum
resolves by path first — once a request matches one of these, a method that
route doesn't handle gets a 405 from *that* route, not a fallthrough to the
dynamic dispatcher. So **any** method on one of these paths is unreachable
for a provider, not just Soma's own method for it (a provider declaring
`GET /v1/greet` is exactly as dead as one declaring `POST /v1/greet`,
despite Soma's own `/v1/greet` being a POST). `lint` reserves all seven
paths — method-independent for the literal six, and pattern-matched for any
literal `/v1/tools/<single-segment>` path — to catch this before it ships.

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
MCP servers also refresh when clients list/get prompts or list/read
resources, so a newly dropped Markdown prompt or `providers/resources/` file
appears without rebuilding the binary.

If a refresh fails — an invalid file, a name/URI collision, a symlink that
escapes the provider root — the server logs a warning and keeps serving the
last valid snapshot rather than failing every other, unrelated, already-loaded
provider's requests too.

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

## Markdown Prompts

Drop a `.md` file into the provider directory to expose it as an MCP prompt. The
file stem becomes the prompt name after lowercasing and replacing punctuation
with hyphens, so `Code Review.md` becomes `code-review`. The first `# Heading`
becomes the prompt description when present, and the full Markdown file is
returned as the prompt message. A `README.md` in the provider directory is
never treated as a prompt.

## Resources

Drop a file into `providers/resources/` (recursive) to expose it as an MCP
resource. Every file that isn't a `.ts` reader becomes a static resource;
`.ts` files become dynamic resource templates.

**Static** — the path relative to `resources/`, minus the extension, becomes
the URI:

```text
providers/resources/api/schema.json  ->  soma://resources/api/schema
```

`name` is the joined path segments, `description` comes from the first `#
Heading` for `.md` files (a generated fallback otherwise), and `mime_type` is
inferred from the extension. Files over 10 MiB are rejected.

**Dynamic** — `.ts` files export `async function read(input)` and use
bracket segments for path parameters, the same convention as
`providers/prompts/`'s naming but applied to directory structure:

```text
providers/resources/service/[name].ts       -> soma://resources/service/{name}
providers/resources/repo/file/[...path].ts  -> soma://resources/repo/file/{path}
```

`input` is `{ uri, params, query }`; the reader returns `{ text, mimeType? }`,
`{ json }`, or `{ blob, mimeType }`. Dynamic readers run through the same
sandboxed Node sidecar `ai-sdk` tool providers use — no network or filesystem
access beyond what the script itself does, no inherited environment
variables.

Both static and dynamic resource files are recursively discovered with a
path-traversal check: a symlink whose target resolves outside the
`resources/` root fails the directory scan rather than being silently loaded.
See `docs/contracts/drop-in-provider-layout.md` for the full contract,
including URI-matching precedence and ambiguity rules.

## Examples

See `examples/providers/`, including `examples/providers/resources/` for the
structured resources layout.
