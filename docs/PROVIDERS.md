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

## Python Providers

Plain `.py` providers can keep using a raw `PROVIDER` dictionary plus public
functions. Soma also embeds a dependency-free `soma_provider` helper, so deployed
providers need no package install or `PYTHONPATH` configuration. Python projects
can optionally install the matching `soma-provider` package for development, IDE
support, and unit tests; both modes expose the same import and metadata contract:

```python
from soma_provider import Context, provider, tool

PROVIDER = provider(name="math-tools", kind="python")

@tool(
    name="sum-values",
    title="Sum values",
    input_schema={
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "a": {"type": "integer"},
            "b": {"type": "integer"},
        },
        "required": ["a", "b"],
    },
    output_schema={"type": "integer"},
    cli={"aliases": ["sum"]},
    meta={"owner": "platform"},
)
def add(a: int, b: int, ctx: Context) -> int:
    """Add two values. Context is supplied by Soma."""
    return a + b
```

The decorator returns the original function. Explicit decorator fields win over
function-name/docstring/annotation inference, which in turn wins over adapter
defaults. A decorated `name` is the action used by catalog and dispatch. Explicit
`input_schema` skips annotation resolution, while positional-only parameters remain
invalid because JSON object keys are passed as keyword arguments. `cli` shallowly
overlays the generated `{"enabled": true, "command": <name>}` value, and
`meta.python.adapter` is reserved for the host.

Decorator metadata maps to the existing provider tool contract: `name`,
`description`, `title`, `input_schema`, `output_schema`, `scope`, `destructive`,
`requires_admin`, `cost`, `env`, `limits`, `mcp`, `rest`, `cli`, `palette`, `ui`,
`examples`, and `meta`. Rust validates the fully resolved manifest before the tool
is registered. Legacy public-function discovery, explicit `TOOLS = []`, LangChain,
and LlamaIndex providers remain unchanged.

Use `provider(...)` to build a validated `PROVIDER` mapping. A parameter
annotated as `soma_provider.Context` is omitted from the public input schema and
injected during dispatch. Its immutable `request` carries the request ID,
provider, action, surface, and snapshot.

Soma can prepare PEP 723 providers into immutable, content-addressed `uv`
environments before catalog import. The lifecycle is disabled by default;
enable it through `[python.environment]` or
`SOMA_PYTHON_ENVIRONMENT_ENABLED=true` and supply every identity-bearing input.
Enabled but incomplete configuration fails startup. With the lifecycle
disabled, one-shot startup uses `SOMA_PYTHON_COMMAND` or ambient `python3`, and
persistent startup uses its ambient default interpreter.

The configured runtime fields must describe `python_executable`, and
`uv_version` must describe `uv_program`. They form part of the cache identity.
Soma verifies the exact SDK wheel bytes against `sdk_wheel_sha256` before
publishing cache state. Both one-shot and persistent modes then use the
prepared environment's interpreter.

```toml
[python.environment]
enabled = true
cache_root = "/var/cache/soma"
uv_program = "/usr/local/bin/uv"
uv_version = "0.11.31"
python_executable = "/usr/bin/python3.12"
runtime_implementation = "cpython"
runtime_version = "3.12.4"
runtime_platform = "linux-x86_64"
wheel_platform_tag = "manylinux_2_17_x86_64"
sdk_wheel = "/opt/soma/soma_provider-0.2.0-cp38-abi3-manylinux_2_17_x86_64.whl"
sdk_wheel_sha256 = "<64 hexadecimal characters>"
offline = false
update = false
policy_version = 2
```

`policy_version` must match the environment-plan version supported by the
running Soma binary. `update = true` resolves a new immutable candidate at
startup; it cannot be combined with `offline = true`. Cache roots are created
private on Unix and existing roots that grant group or other access are
rejected; the service user must own the root and its parent path must prevent
untrusted replacement. Managed environments currently fail closed on Windows
until equivalent private-cache ACL enforcement is available. Startup and every
later preparation verify the SDK digest, wheel target, configured Python, and
the prepared interpreter before activation. Cache misses and explicit updates
also verify `uv` immediately before mutation. Warm starts probe Python but do
not require the `uv` executable to remain installed.

Operators use the same application actions from CLI, MCP, or REST:

| Action | Scope | Purpose |
|---|---|---|
| `python_environment_status` | `soma:write` + admin | Inventory ready, incomplete, corrupt, and staging cache entries without importing provider code. |
| `python_environment_prune_plan` | `soma:write` + admin | Produce a bounded conservative prune plan. |
| `python_environment_prune` | `soma:write` + confirmation | Apply the bounded plan with race-safe revalidation. |
| `python_environment_repair` | `soma:write` | Repair the exact environment for a managed provider path. |
| `python_environment_update` | `soma:write` | Prepare, validate, and atomically activate a new immutable candidate. |

For example:

```bash
soma python_environment_status
soma python_environment_prune_plan --json \
  '{"stale_before_unix_seconds": 1722384000, "max_entries": 25}'
soma python_environment_prune --confirm --json \
  '{"stale_before_unix_seconds": 1722384000, "max_entries": 25}'
soma python_environment_repair --json '{"provider_path":"providers/example.py"}'
soma python_environment_update --json '{"provider_path":"providers/example.py"}'
```

Repair and update paths must resolve to regular, non-symlink `.py` files under
the configured provider root. Update never mutates the active environment: it
publishes a new content-addressed candidate and swaps the registry only after
candidate validation succeeds.

### Persistent Python runner

One-shot execution remains the default and rollback path. Set
`SOMA_PYTHON_RUNNER_MODE=persistent` to prestart one supervised, serial worker
per Python provider. Persistent mode requires the matching `soma-provider`
wheel in the selected interpreter because workers start with
`python -I -m soma_provider.runner`; startup fails closed instead of silently
falling back to one-shot.

Persistent workers connect to an ephemeral loopback TCP listener and
authenticate with a per-launch token before using the length-prefixed JSON
control protocol. The child process's stdin and stdout are redirected to the
platform null device and are not used for control. Provider stdout is redirected
to the continuously drained stderr stream; the host converts it into bounded,
sequenced structured log entries and redacts sensitive diagnostics before
operator-visible retention. The host negotiates
features, describes and health-checks every candidate before publishing it,
rejects concurrent calls with
`python_provider_busy`, kills the worker process tree on timeout or protocol
failure, and permits a later serialized restart within the configured
restart-window budget. Repeated failures quarantine that provider generation.
Source files must be regular non-symlink files and are hashed immediately
before launch and again after describe. Active work can be cancelled
deterministically by terminating the complete worker process tree; later work
starts a clean worker without replay.

Persistent mode deliberately rejects provider- or tool-level runtime
environment declarations with `python_persistent_env_unsupported`. It does not
forward actor scopes or trace context, and HTTP, secrets, state, logging,
metrics, progress, and cooperative broker cancellation remain unavailable.
Host cancellation is enforced at the process boundary and therefore also stops
uninterruptible synchronous handlers and descendants.

Worker and generation controls use the same application authorization on every
surface:

| Action | Scope | Purpose |
|---|---|---|
| `python_worker_status` | `soma:write` + admin | Inspect running/busy/quarantined state, restart counts, generation identity, and bounded redacted logs. |
| `python_worker_cancel` | `soma:write` + confirmation | Cancel active work by terminating the worker process tree. |
| `python_worker_reset` | `soma:write` + confirmation | Clear crash-loop quarantine and permit a fresh worker. |
| `python_generation_status` | `soma:read` | Inspect the active generation and bounded rollback window. |
| `python_generation_rollback` | `soma:write` + confirmation | Atomically reactivate a retained provider/environment/worker generation. |

Filesystem refresh uses a single coalescing preparation lane and a short
debounce window. Candidate catalogs, immutable environments, and persistent
workers are prepared and health-checked before one atomic registry publication.
The last three generations are retained for rollback; older generations are
drained and retired outside registry locks. Python generations snapshot the
complete non-symlink provider tree, including adjacent data files, with a
4,096-file/64 MiB bound; snapshots are reclaimed after the last active,
retained, or in-flight provider releases them. New requests never route to a
retained generation, while calls already holding that generation finish on it.

The main controls are:

| Variable | Default |
|---|---:|
| `SOMA_PYTHON_RUNNER_MODE` | `one-shot` |
| `SOMA_PYTHON_RUNNER_STARTUP_TIMEOUT_MS` | `10000` |
| `SOMA_PYTHON_RUNNER_REQUEST_TIMEOUT_MS` | `10000` |
| `SOMA_PYTHON_RUNNER_SHUTDOWN_GRACE_MS` | `2000` |
| `SOMA_PYTHON_RUNNER_MAX_RESTARTS` | `3` |
| `SOMA_PYTHON_RUNNER_RESTART_WINDOW_MS` | `60000` |
| `SOMA_PYTHON_RUNNER_RESTART_BACKOFF_MS` | `250` |
| `SOMA_PYTHON_RUNNER_MAX_STDERR_BYTES` | `65536` |
| `SOMA_PYTHON_RUNNER_MAX_PENDING_BYTES` | `524288` |
| `SOMA_PYTHON_RUNNER_MAX_WORKERS` | `32` |
| `SOMA_PYTHON_RUNNER_MAX_CANDIDATE_STARTS` | `4` |

Immutable-environment controls map directly to the TOML example above:
`SOMA_PYTHON_ENVIRONMENT_ENABLED`, `CACHE_ROOT`, `UV_PROGRAM`, `UV_VERSION`,
`PYTHON_EXECUTABLE`, `RUNTIME_IMPLEMENTATION`, `RUNTIME_VERSION`,
`RUNTIME_PLATFORM`, `WHEEL_PLATFORM_TAG`, `SDK_WHEEL`, `SDK_WHEEL_SHA256`, and
`OFFLINE`, `UPDATE`, and `POLICY_VERSION`, all with the
`SOMA_PYTHON_ENVIRONMENT_` prefix.

Python providers are trusted executable code. Environment clearing and bounded
sidecar I/O are safety controls, not an OS sandbox; imported code retains the
filesystem, network, and process authority of the Soma service account. See
[ADR 0013](./adr/0013-python-provider-authoring-boundary.md) for the authoring and
Python-to-WASM graduation boundary.

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

See `examples/providers/`, including `examples/providers/python/` for minimal,
decorated Context, async, Pydantic, LangChain, and LlamaIndex providers, plus
`examples/providers/resources/` for the structured resources layout.
