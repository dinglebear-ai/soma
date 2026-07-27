# Dynamic Provider Runtime

## Status

Draft specification for the provider-based runtime direction.

## Goal

Turn Soma into a batteries-included runtime shell where users focus on
creating and testing tools while the server supplies the repeated platform work:

- MCP server with one compact action-dispatched service tool
- traditional REST routes backed by provider metadata
- dynamic CLI commands
- provider-aware OpenAPI
- Palette desktop UI and Aurora web UI
- generated client recipes
- generated `SKILL.md`, plugin manifests, and marketplace metadata
- auth, setup, doctor, logging, tracing, redaction, validation, limits, and reload

The runtime must support native Rust actions and runtime-loadable providers
without requiring edits to the MCP, REST, CLI, Palette, or plugin surface crates
for every new business capability.

## Non-Goals

- Do not bring back a REST action envelope such as `the retired REST action-envelope route`.
- Do not require users to hand-write large JSON manifests for the happy path.
- Do not expose MCP-native prompts/resources/tasks/elicitation through REST or
  CLI by default.
- Do not let provider-specific protocol types leak into the public runtime
  contract.
- Do not silently rewrite committed generated files from normal server mode.

## Runtime Model

The provider registry is the source of truth.

```text
ProviderRegistry
  StaticRustProvider
  OpenApiProvider
  McpProvider
  AiSdkToolProvider
  PythonFunctionProvider
  LangChainProvider
  LlamaIndexProvider
  WasmProvider
```

Each provider returns a `ProviderManifest` and implements execution for the
capabilities it owns. The registry validates manifests, normalizes defaults,
detects conflicts, computes a fingerprint, and exposes a single merged catalog
to every generic surface.

`StaticRustProvider` is the proof path for the registry. The built-in Rust
actions move through the provider registry first so the public MCP, REST, CLI,
OpenAPI, docs, plugin, and Palette surfaces prove the immutable snapshot and
dispatch rules before dynamic provider code is trusted.

```text
ProviderManifest
  -> MCP tools/prompts/resources/tasks/elicitation
  -> REST dynamic routes
  -> CLI dynamic commands
  -> OpenAPI document
  -> Palette manifest
  -> Aurora UI generation inputs
  -> generated SKILL.md and plugin manifests
  -> doctor/setup/env requirements
```

## Provider Types

### Static Rust Provider

Wraps built-in Rust service actions. This is the provider for polished native
features and reusable crates shipped with the binary.

Native Rust actions still require a Rust rebuild, but they should not require
per-action edits in MCP, REST, CLI, Palette, or plugin code once they are
registered with the provider registry.

### OpenAPI Provider

Recommended default provider for wrapping existing APIs.

An OpenAPI provider loads an OpenAPI spec, filters and renames operations,
normalizes them into provider tools, and executes them through an HTTP client.
It should be the easiest path for users with an existing API:

```toml
[providers.github]
kind = "openapi"
spec = "./openapi/github.json"
base_url = "https://api.github.com"
auth.env = "GITHUB_TOKEN"
include_tags = ["repos", "issues"]
name_prefix = "github"
```

The OpenAPI provider should not paste the upstream OpenAPI document wholesale
into this server's OpenAPI. It must expose the curated, auth-wrapped, filtered,
and renamed operations through the provider registry first.

### TypeScript AI SDK Provider

Loads `.tool.ts` files through a Node or Bun sidecar. This is the provider for
fast custom logic and AI-native glue code.

Minimal authoring should look like:

```ts
import { defineTool } from "@soma/tools";
import { z } from "zod";

export default defineTool({
  description: "Get weather for a city.",
  input: z.object({
    city: z.string(),
  }),
  execute: async ({ city }) => ({ city, celsius: 20 }),
});
```

The provider infers the action name from `weather.tool.ts`, converts Zod schemas
to JSON Schema, and exposes the action everywhere.

### Python Function Provider

Loads `.py` files through a Python sidecar. This is the lowest-friction Python
path for users who already have functions and do not need a framework adapter.

Minimal authoring should look like:

```python
PROVIDER = {"name": "math-tools", "kind": "python"}

def add(a: int, b: int) -> int:
    """Add two integers."""
    return a + b

async def echo(message: str) -> dict:
    """Echo a message."""
    return {"message": message}
```

When `TOOLS` is absent, the provider autodiscovers public functions defined by
the module. Private names are ignored. Function signatures are converted into
JSON Schema for common Python annotations such as `str`, `int`, `float`, `bool`,
`dict`, `list`, and typed lists.

For explicit metadata, Soma embeds a dependency-free authoring helper into the
sidecar before importing the provider:

```python
from soma_provider import tool

@tool(
    name="sum-values",
    input_schema={
        "type": "object",
        "properties": {
            "a": {"type": "integer"},
            "b": {"type": "integer"},
        },
        "required": ["a", "b"],
        "additionalProperties": False,
    },
    cli={"aliases": ["sum"]},
)
def add(a, b):
    return a + b
```

The module is registered as `soma_provider` without installation or
`PYTHONPATH`. The decorator preserves function identity and records versioned
JSON-compatible metadata. Explicit decorator values win over existing inference
and adapter defaults. Explicit schemas avoid eager annotation resolution, while
signature validation still rejects positional-only parameters. The resolved tool
is validated against the canonical Rust manifest before registration.

The Rust adapter and one-shot Python worker use a private schema-versioned NDJSON
request/response envelope with a request ID and explicit `catalog`/`call` mode.
Responses must match the request version, ID, and mode. Envelope capture has
bounded headroom, and extracted payloads remain subject to their configured byte
limits.

A separate persistent-runner protocol seam is versioned independently from that
compatibility envelope. Its dedicated control channel uses a four-byte
big-endian length followed by bounded UTF-8 JSON. The worker initiates a
major/minor handshake; matching majors negotiate the lower minor and the
intersection of advertised features. Typed messages cover `describe`, `invoke`,
`cancel`, `health`, `drain`, `shutdown`, brokered `host.*` calls, stable
invocation states, and redacted error codes. Invocation envelopes carry the
request and invocation IDs, deadline, trace, actor/scopes, cancellation token,
and generation required for at-most-once dispatch. Rust and dependency-free
Python codecs consume shared golden fixtures. This seam is not the active
execution engine yet; the one-shot bridge remains in place until the supervisor
phase lands.

Python provider files are trusted code, not inert configuration. Soma imports
the module to derive the catalog, so top-level Python runs during provider
refresh. The sidecar starts with a cleared environment and receives only
declared provider/tool env values during tool execution. Catalog import does
not receive provider env; provider code must read secrets inside tool functions
instead of at module import time. A cleared environment and bounded I/O do not
make the sidecar an OS sandbox; Python code retains the filesystem, network, and
process authority of the Soma service account. The request `snapshot_id` binds
dispatch to catalog metadata but does not attest the Python file or its imported
dependencies; source drift can execute at call time until the registry refreshes.

A Python implementation graduates to WASM by preserving its provider manifest,
schemas, action names, and surface overlays while replacing the implementation.
Soma does not define arbitrary Python-to-WASM transpilation. See
[ADR 0013](../adr/0013-python-provider-authoring-boundary.md).

### Python LangChain and LlamaIndex Providers

Loads `.py` files through a Python sidecar. These providers are for reusing
existing Python tool ecosystems rather than inventing a Soma-specific Python
tool API.

Minimal LangChain authoring should look like:

```python
from langchain_core.tools import tool

PROVIDER = {"name": "weather-tools", "kind": "langchain"}

@tool
def get_weather(city: str) -> dict:
    """Get weather for a city."""
    return {"city": city, "celsius": 20}

TOOLS = [get_weather]
```

Minimal LlamaIndex authoring should look like:

```python
from llama_index.core.tools import FunctionTool

PROVIDER = {"name": "math-tools", "kind": "llamaindex"}

def add(a: int, b: int) -> int:
    """Add two integers."""
    return a + b

TOOLS = [FunctionTool.from_defaults(add, name="add")]
```

The provider imports the module, reads `PROVIDER` and `TOOLS`, converts
LangChain/LlamaIndex tool metadata into provider tool schemas, and executes tool
calls in a Python sidecar. `SOMA_PYTHON_COMMAND` may point at a virtualenv
or `uv`/Python wrapper when the default `python3` is not the right interpreter.
These provider files have the same trusted-code behavior as plain Python
providers: module import happens during catalog refresh, while tool execution
runs with only declared environment values. Framework tools should avoid
constructing env-dependent clients at module import time.

### WASM Provider

Loads sandboxed `.wasm` modules. This is the provider for portable runtime
plugins with stronger host control than arbitrary scripts.

The first version may use a JSON-in/JSON-out ABI:

```text
call(input_json, context_json) -> output_json
```

The manifest may live beside the module or be exported by the module. Tooling
should prefer generated or inspected manifests over hand-written manifests.

## Provider Manifest Contract

The machine-readable contract lives in
[`docs/contracts/provider-manifest.schema.json`](../contracts/provider-manifest.schema.json).

Every provider ultimately normalizes into this shape. Authoring helpers may let
users write less, but the host validates the resolved manifest.

Required top-level concepts:

- provider identity
- tools/actions
- prompts
- resources
- tasks
- elicitation forms
- environment requirements
- host capabilities
- surface overlays for MCP, REST, CLI, Palette, Aurora, docs, and plugins

## MCP Support

MCP remains the compact agent surface.

- expose one service tool for regular tool/action calls
- route `{ "action": "name", ... }` through the provider registry
- list provider tools through the tool schema/action catalog
- support provider-owned prompts
- support provider-owned resources and resource templates
- reserve room for tasks
- support elicitation declarations and flows

MCP-native primitives are not automatically mirrored to REST or CLI. REST and CLI
are default surfaces for tools/actions only.

## REST Support

REST stays traditional from the client perspective.

```text
POST /v1/weather
GET  /v1/status
```

The implementation may use a generic internal dynamic route such as
`/v1/{action}`, but public behavior must look like direct typed routes and must
not reintroduce a public action envelope.

REST overlays may define:

- enabled/disabled
- method
- path
- path params
- query params
- request body schema
- response schema
- tags
- summary/description
- content types
- status codes
- streaming mode
- pagination metadata
- deprecation metadata

## CLI Support

The CLI keeps built-in infrastructure commands compiled in:

```text
serve
mcp
doctor
watch
setup
tools
providers
openapi
help
```

All provider tools become dynamic subcommands:

```bash
example weather --city Paris
example qdrant-search --query "rmcp tracing"
example summarize --text ./notes.md
soma help weather
```

The CLI derives flags and optional positionals from the provider schema and CLI
overlay. It should include a JSON escape hatch:

```bash
example weather --json '{"city":"Paris"}'
```

## Palette Support

Palette should become a generic desktop shell for the provider runtime.

It should load a palette manifest derived from the provider registry and render:

- dynamic command entries
- schema-driven forms
- result previews
- provider groups/categories
- env/setup issues
- permissions and auth state
- OpenAPI-backed routes
- local built-ins such as file explorer, GitHub repo viewer, browser, and terminal

Palette built-ins are local capabilities and should not automatically become
remote REST or MCP tools.

Built-ins should be enable/disable capable with explicit permissions:

- file explorer: roots, read-only/read-write, hidden file policy
- GitHub repo viewer: allowed repos/orgs and token source
- browser: allowed origins and cookie/session policy
- terminal: disabled by default, cwd restrictions, command allowlist, env redaction

## Aurora and shadcn Registry Support

Aurora should be the default UI registry layer.

The provider manifest should drive generated or selected Aurora UI:

- tool/action explorer
- schema display
- env var setup
- permission prompts
- OAuth/auth blocks
- terminal and logs
- data tables
- tool-call history
- code/editor/artifact previews

shadcn registry artifacts can carry files, dependencies, registry dependencies,
CSS variables, docs, env var templates, metadata, blocks, hooks, pages, and
components. The runtime should use this for scaffolded web UI and Palette UI
rather than hand-authoring every tool screen.

## OpenAPI and Client Generation

The runtime publishes one canonical OpenAPI document for all REST-exposed
provider tools:

```text
Static Rust actions
+ OpenAPI provider actions
+ TypeScript tools
+ WASM tools
= GET /openapi.json
```

The live endpoint is generated from the current provider registry.

Snapshot mode is explicit:

```bash
example openapi --write docs/generated/openapi.json
example openapi --check docs/generated/openapi.json
```

The generated OpenAPI should contain a registry fingerprint:

```json
{
  "x-soma": {
    "provider_fingerprint": "sha256:...",
    "providers": []
  }
}
```

The fingerprint should include provider manifests, OpenAPI spec files, WASM
manifests, TypeScript tool metadata, server config affecting exposed actions,
and runtime schema version.

Client generation should be recipes and helper commands by default, not stale
SDK packages checked into Soma. Recommended TypeScript default:

```bash
npx openapi-typescript http://localhost:40060/openapi.json -o src/generated/api.d.ts
npm i openapi-fetch
```

## Plugin, Marketplace, and Skill Generation

Provider manifests should generate agent-facing packaging artifacts:

- `SKILL.md`
- Claude plugin metadata
- Codex plugin metadata
- Gemini extension metadata
- marketplace metadata
- optional MCP config where the target ecosystem expects it
- MCP Registry/server metadata

The generated `SKILL.md` should include:

- server purpose
- when to use the server
- tools/actions
- prompts/resources/tasks/elicitation
- input examples
- CLI examples
- REST examples
- env/auth prerequisites using server-prefixed env vars
- troubleshooting from provider validation and `doctor`

## Environment Variables and Secrets

Provider tools declare logical env requirements. The host resolves them with
server-aware precedence:

1. server-prefixed env var, e.g. `LAB_OPENAI_API_KEY`
2. unprefixed env var, e.g. `OPENAI_API_KEY`, only if allowed
3. provider/tool-specific secret source

`doctor` reports missing values using the server-prefixed form by default:

```text
summarize requires LAB_OPENAI_API_KEY
qdrant-search requires LAB_QDRANT_URL
```

Secrets and raw inputs must not be logged by default.

## Capability and Security Model

Dynamic providers are security-sensitive.

Required default posture:

- default-deny host capabilities
- schema validation before execution
- execution timeout
- response-size limit
- auth/scope checks before execution
- destructive-action confirmation
- duplicate action detection
- no raw input/secret logging
- structured provider errors
- provider failure isolation
- explicit filesystem/network/env capability declarations

OpenAPI providers are generally safer than code providers because they can only
call declared HTTP operations, but they still need auth, filtering, rate limits,
and route curation.

WASM, TypeScript, and Python providers must run under explicit capability
policy.

## Reload and Staleness

Runtime provider reload should update the in-memory registry and live
`/openapi.json`.

Committed generated artifacts should only update through explicit commands.

```bash
example providers reload
example openapi --check
example openapi --write
example plugin generate --check
example skill generate --write
```

Provider reload should report:

- added/removed/changed providers
- added/removed/changed tools
- duplicate action conflicts
- invalid schemas
- missing env/capabilities
- stale generated artifacts

## Implementation Phases

1. Define provider manifest Rust types and validation.
2. Introduce provider registry and `StaticRustProvider`.
3. Convert MCP, REST, and CLI to generic provider-driven dispatch.
4. Add unified OpenAPI generation and fingerprint checks.
5. Add OpenAPI provider as the recommended default dynamic provider.
6. Add TypeScript AI SDK sidecar provider.
7. Add Python LangChain and LlamaIndex sidecar providers.
8. Add WASM provider.
9. Add provider-driven Palette manifest and generic Palette shell.
10. Add Aurora/shadcn UI generation.
11. Add provider-driven `SKILL.md`, plugin, marketplace, and Gemini generation.

## Acceptance Principles

- Adding a Rust-native action should not require edits to MCP, REST, CLI, Palette,
  OpenAPI, docs, or plugin surface files.
- Adding an OpenAPI provider should not require a Rust rebuild.
- Dropping in valid `.tool.ts`, `.py`, or `.wasm` providers should not require a
  Rust rebuild.
- The happy path should infer names, routes, commands, schemas, docs, and UI from
  convention.
- Advanced configuration should be additive and optional.
