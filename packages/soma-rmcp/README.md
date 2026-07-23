# Soma

`Soma` is a batteries-included RMCP server runtime and shipping binary
for bringing new agent capabilities online with as little custom Rust as
possible. It locks in the production patterns that every server in the family
keeps rediscovering: one compact MCP tool, stdio and Streamable HTTP transports,
CLI parity, direct REST routes, auth/OAuth, observability, plugin packaging,
web fallback, Docker/runtime samples, generated contracts, and release
automation.

The repository can still scaffold a renamed project, but Soma is now a shipped
runtime first. The default product path is to run `soma` in an explicit mode,
drop provider files into `providers/` (or point `SOMA_PROVIDER_DIR`
elsewhere), and let the provider registry project those capabilities across MCP,
CLI, REST, OpenAPI, Palette summaries, generated docs, and plugin metadata.
Provider manifests also carry MCP-native prompt, resource, task, and elicitation
metadata for the registry contract. Scaffolding is the path for creating a new
distributable repo with the same locked-in runtime.

**30-second path:** install the `soma` binary -> `soma status` ->
`npx -y soma-rmcp mcp` from an MCP client -> call the `soma` MCP tool through
`tools/call` with `{"action":"status"}`.

**Status:** production RMCP runtime. Write-capable provider actions are
allowed only when the provider declares them and destructive actions are gated.

**Not for:** an unauthenticated public gateway, a replacement for upstream
service authorization, arbitrary untrusted code execution, or a multi-tenant
security boundary by itself.

## Contents

- [Naming](#naming)
- [Capabilities And Boundaries](#capabilities-and-boundaries)
- [Install](#install)
- [Quickstart](#quickstart)
- [Client Configuration](#client-configuration)
- [Runtime Surfaces](#runtime-surfaces)
- [MCP Tool Reference](#mcp-tool-reference)
- [CLI Reference](#cli-reference)
- [Configuration](#configuration)
- [Authentication](#authentication)
- [Safety And Trust Model](#safety-and-trust-model)
- [Architecture](#architecture)
- [Distribution Contract](#distribution-contract)
- [Development](#development)
- [Verification](#verification)
- [Deployment](#deployment)
- [Troubleshooting](#troubleshooting)
- [Related Servers](#related-servers)
- [Documentation](#documentation)
- [License](#license)

## Naming

Soma is the runtime product first and the template/export source second.
Generated projects replace these names during scaffold post-processing, but the
shipped `soma` command is the source of truth for product behavior.

| Surface | Soma value | Generated-project pattern |
|---|---|---|
| Repository | `dinglebear-ai/soma` | `<service>-rmcp` or a documented product exception |
| Rust crate/package | `soma` | service-specific crate names |
| Canonical binary | `soma` | usually `r<service>` or the product name |
| npm package | `soma-rmcp` | `<service>-rmcp` |
| MCP tool | `soma` | usually `<service>` |
| Env prefix | `SOMA_*` | generated service prefix |

## Capabilities And Boundaries

| Path | Use when | You author | Runtime supplies |
|---|---|---|---|
| Drop-in provider | You can describe a capability as a manifest, script, WASM module, OpenAPI operation, or upstream MCP call. | Files under `providers/` with tools, prompts, resources, env needs, capability grants, and surface overlays. | MCP tool dispatch, dynamic CLI commands, direct REST routes, schema validation, auth policy, refresh, OpenAPI/Palette summaries, generated docs, and plugin metadata. |
| Static Rust provider | The capability needs native Rust, tight integration, or reusable crates. | A Rust provider/action registered with the provider registry. | The same MCP/CLI/REST/docs/plugin projection without per-surface rewrites. |
| Scaffolded product | You need a renamed repository, package identity, ports, plugins, Docker labels, and release metadata. | A `scaffold_intent` payload or `cargo xtask scaffold` options. | A compiling product repo, scaffold report, cargo-generate post-processing, and scaffold/export verification checks. |
| Custom profile | You need a narrower binary or deployment shape. | Cargo feature selection. | The same runtime crates behind `local-adapter`, `server`, and `full` profiles. |

## Batteries Included

- One compact MCP service tool (`soma`) with `action` dispatch, so agent tool
  lists stay small even as provider catalogs grow.
- One canonical binary: `soma` with explicit `serve`, `mcp`, and CLI modes for
  REST API, Streamable HTTP MCP, stdio MCP, optional web UI, and local actions.
- Dynamic provider loading from `.json`, `.ts`, `.py`, `.wasm`, and `.md`
  files, plus native Rust providers and upstream MCP/OpenAPI provider kinds.
  A structured `providers/{tools,prompts,resources}/` layout is supported
  alongside root-level files, including path-derived MCP resources (static
  files and dynamic `.ts` readers) with a path-traversal trust boundary.
- Provider manifest contracts for tools, prompts, resources, tasks,
  elicitation forms, env requirements, capability grants, and surface overlays.
- Shared validation, destructive-action confirmation, auth/scope enforcement,
  response limits, redaction, logging, metrics, generated OpenAPI, generated
  provider surface docs, plugin manifests, setup, doctor, and release tooling.

Soma owns the runtime projection, validation, auth policy, packaging, generated
metadata, and scaffold automation. Provider code owns service-specific behavior
and credentials. Upstream services own their own authorization and data model.
Soma deliberately refuses to make credentials part of tool-call input and does
not turn provider manifests into an unrestricted remote execution boundary.

## Install

Use the npm launcher when an MCP client expects an `npx` command. The package is
a launcher for the Rust binary; install `soma` first or set `SOMA_BIN` to its
absolute path.

```bash
npx -y soma-rmcp mcp
```

Use Cargo while developing the repo:

```bash
cargo run --bin soma -- mcp
cargo run --bin soma -- serve
```

Release builds publish GitHub Release binaries, Docker/OCI metadata, the
`soma-rmcp` npm launcher, MCP registry metadata, and plugin package files from
the same release component.

## Product Profiles

Choose the amount of surface area you want without changing the provider authoring
model.

| Target | Best fit | Default profile | Includes |
|---|---|---|---|
| Local agent adapter | Thin wrapper over dropped providers or an upstream API | `local-adapter` | CLI + stdio MCP in one local binary. No REST/Web mirror by default. |
| Shared API/MCP server | Service used by multiple clients or a gateway | `server` | CLI + REST API + Streamable HTTP MCP + stdio MCP + health/status routes + auth-capable runtime. |
| Full application platform | App owns state, jobs, dashboards, workflows, or human UI | `full` | `server` plus embedded web UI, OAuth, observability, and plugin support. |
| CLI-only or custom local tool | Scripts, operator utilities, one-machine tools | Custom feature set, usually starting from `cli` | CLI parser and shared service layer. The stock packaged local binary uses `local-adapter`, so CLI-only products may prune MCP or adjust binary feature gates. |

Lower-level Cargo features are available when you need a custom shape:

| Feature | Purpose |
|---|---|
| `cli` | CLI shim and command parsing. |
| `mcp` | MCP tool, schema, resource, prompt, and scope layers. |
| `mcp-stdio` | Local stdio MCP transport. |
| `api` | REST handlers and OpenAPI-backed business routes. |
| `auth` | Shared auth policy and bearer-token enforcement. |
| `oauth` | Google, Authelia, and GitHub OAuth/OIDC plus JWT issuance on top of `auth`. |
| `mcp-http` | Streamable HTTP MCP mounted in Axum. |
| `web` | Embedded static web UI fallback. |
| `observability` | Metrics/tracing hooks. |
| `plugin` | Plugin setup/support helpers. |
| `local-adapter` | Lean local binary: `cli` + `mcp-stdio`. |
| `server` | Deployable HTTP runtime profile: `cli` + `api` + HTTP MCP + stdio MCP. |
| `full` | Complete platform profile: local adapter, server, web, OAuth, observability, and plugin support. |

## Quickstart

Run the product as-is:

```bash
git clone https://github.com/dinglebear-ai/soma
cd soma

# Full platform mode: REST API + HTTP MCP + web fallback on :40060
cargo run --bin soma -- serve

# Local binary: stdio MCP
cargo run --bin soma -- mcp

# Local binary: CLI
cargo run --bin soma -- greet --name Alice
```

Useful smoke checks:

```bash
curl http://localhost:40060/health
cargo run --bin soma -- status
cargo run --bin soma -- doctor
```

Call the MCP endpoint directly:

```bash
curl -s -X POST http://localhost:40060/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"soma","arguments":{"action":"greet","name":"Alice"}}}'
```

## Drop In A Provider

The fastest path for a new server is provider-first. Add a provider manifest or
module to `providers/`, then run the same binary. Use `SOMA_PROVIDER_DIR`
when the provider catalog should live outside the working directory.

```bash
mkdir -p providers
cat > providers/hello-local.json <<'JSON'
{
  "schema_version": 1,
  "provider": {
    "name": "hello-local",
    "kind": "static-rust",
    "title": "Hello Local"
  },
  "tools": [
    {
      "name": "hello_local",
      "description": "Return a deterministic hello payload from a dropped provider.",
      "input_schema": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "name": { "type": "string" }
        }
      },
      "cli": {
        "enabled": true,
        "command": "hello-local"
      },
      "rest": {
        "enabled": true,
        "method": "POST",
        "path": "/v1/hello-local"
      },
      "meta": {
        "result": {
          "message": "hello from a dropped provider"
        }
      }
    }
  ]
}
JSON
```

Call it through the dynamic CLI surface:

```bash
cargo run --bin soma -- hello-local --name Alice
```

Run the server and call the same provider over REST and MCP:

```bash
cargo run --bin soma -- serve

curl -s -X POST http://localhost:40060/v1/hello-local \
  -H "Content-Type: application/json" \
  -d '{"name":"Alice"}'

curl -s -X POST http://localhost:40060/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"soma","arguments":{"action":"hello_local","name":"Alice"}}}'
```

Plain Python functions can also be dropped directly into `providers/`:

```python
PROVIDER = {"name": "math-tools", "kind": "python"}

def add(a: int, b: int) -> int:
    """Add two integers."""
    return a + b
```

When `TOOLS` is absent, public functions defined in the module become tools.
Sync and async functions are supported, and common Python type annotations are
converted into input schemas.

Python provider files are trusted code. Soma imports them during provider
catalog refresh to discover tools, then executes tool calls in a sidecar with a
cleared environment plus only declared provider/tool env values. Catalog import
does not receive provider env; read secrets inside tool functions, not at module
import time.

Provider manifests can declare:

- tools/actions exposed through MCP by default and through CLI/REST when their
  overlays opt in
- MCP-native prompt, resource, task, and elicitation metadata for the provider
  registry contract
- required environment variables and redaction rules
- filesystem, network, browser, terminal, GitHub, and env capability grants
- limits, destructive-action metadata, examples, generated docs, plugin, and UI
  metadata

Supported provider kinds are `static-rust`, `openapi`, `ai-sdk`, `wasm`, `mcp`,
`python`, `langchain`, and `llamaindex`. See
[docs/specs/dynamic-provider-runtime.md](docs/specs/dynamic-provider-runtime.md),
[docs/contracts/provider-manifest.schema.json](docs/contracts/provider-manifest.schema.json),
and [docs/generated/provider-surfaces.md](docs/generated/provider-surfaces.md).

## Scaffold A New Project

Use `cargo xtask scaffold` when the provider-first path needs to become a new
repository with its own crate names, binary names, ports, plugin package,
Docker metadata, release metadata, and docs. It can plan without touching files,
generate with `cargo-generate` plus the Rust post-processor, write
`docs/scaffold-report.md`, and verify the generated export shape.

Plan from a short service name:

```bash
cargo xtask scaffold --name myservice --category upstream-client --port auto --plan
```

Plan from MCP `scaffold_intent` JSON:

```bash
cargo xtask scaffold --intent scaffold-intent.json --plan
```

Generate into an output parent directory:

```bash
cargo xtask scaffold --intent scaffold-intent.json --apply ../generated
```

Verify an existing generated project:

```bash
cargo xtask scaffold --verify ../generated/myservice-mcp
```

Print a path-aware follow-up plan for adapting the generated stub:

```bash
cargo xtask scaffold --adapt-plan ../generated/myservice-mcp
```

Materialize starter artifacts from an action manifest:

```bash
cargo xtask scaffold \
  --write-action-starters ../generated/myservice-mcp \
  --actions actions.json
```

Add starter action snippets:

```bash
cargo xtask scaffold \
  --intent scaffold-intent.json \
  --actions actions.json \
  --plan
```

Example action manifest:

```json
{
  "actions": [
    {
      "name": "list_things",
      "description": "List visible things.",
      "scope": "read",
      "params": [
        { "name": "kind", "type": "string", "required": false }
      ]
    }
  ]
}
```

Use:

- `--category upstream-client` for a lean local adapter around an existing API.
- `--category application-platform` for API + CLI + MCP + web defaults.
- `--no-cargo-check` only when you need fast static verification while iterating.

See [docs/SCAFFOLD.md](docs/SCAFFOLD.md), [docs/CARGO_GENERATE.md](docs/CARGO_GENERATE.md),
and [docs/contracts/scaffold-intent.schema.json](docs/contracts/scaffold-intent.schema.json)
for the full scaffold contract.

## Architecture

The runtime keeps product behavior behind the provider registry. Every external
surface is a thin parser/formatter around the same provider snapshot and service
runtime, so dropping a provider does not require hand-editing MCP, CLI, REST,
OpenAPI, plugin, or docs code.

```text
ProviderRegistry
  crates/soma/application/src/provider_registry.rs
  Validates provider manifests, computes snapshots/fingerprints, indexes tools,
  prompts, resources, CLI commands, REST routes, and MCP primitives.

Provider sources
  crates/soma/application/src/providers/
  Static Rust, file-backed JSON manifests, TypeScript AI SDK sidecars, Python
  LangChain/LlamaIndex sidecars, WASM, OpenAPI-backed providers, and upstream
  MCP providers.

SomaService
  crates/soma/application/src/service.rs
  Built-in product/service logic used by the static Rust provider.

Transport shims
  crates/soma/cli/src/lib.rs        CLI parser and output formatting.
  crates/soma/mcp/src/tools.rs      MCP JSON args to service calls.
  crates/soma/api/src/api.rs        REST extractors to service calls.
  apps/soma/src/routes.rs     Axum router, auth, MCP, API, web fallback.

Built-in action metadata
  crates/soma/domain/src/actions.rs
  Native action metadata, validation, cached catalog/help, and native dispatch.
```

The thin-shim rule is strict:

1. Parse input at the surface.
2. Call the provider registry or service runtime.
3. Return or print the result.

Do not put business rules in CLI, MCP, REST handlers, or `main.rs`.

## Runtime Surfaces

The canonical binary can run the whole app from one executable:

```bash
soma serve       # HTTP server: REST API + Streamable HTTP MCP + web fallback
soma mcp         # stdio MCP transport
soma status      # CLI command through the same binary
```

Local adapter mode is optimized for plugin/local use:

```bash
soma mcp                # stdio MCP transport
soma greet --name Alice # CLI command
soma doctor             # operator pre-flight checks
soma watch              # poll /health and emit state changes
soma setup check        # plugin/appdata setup checks
```

Every explicit runtime mode loads the provider registry. File providers default
to `./providers` and can be moved with `SOMA_PROVIDER_DIR`. CLI startup, MCP
dispatch, and dynamic REST routes refresh file providers before execution, then
enforce the active provider snapshot's schema, surface, scope, capability,
destructive-action, and response-limit rules.

HTTP routes in the server profile:

| Route | Purpose |
|---|---|
| `/mcp` | Streamable HTTP MCP transport. |
| `/health` | Unauthenticated liveness. |
| `/readyz` | Readiness check. |
| `/status` | Public redacted runtime status. |
| `/openapi.json` | Generated REST OpenAPI schema. |
| `/metrics` | Prometheus metrics when built with `observability`. |
| `/v1/capabilities` | REST route inventory. |
| `/v1/greet`, `/v1/echo`, `/v1/status`, `/v1/help` | Direct REST business routes. |
| `/v1/tools/{action}` | Generic REST execution route for dropped provider tools. |
| `/v1/{provider-route}` | Optional provider-declared REST route when a tool supplies a custom REST overlay. |
| `/mcp/.well-known/*` | OAuth metadata when OAuth is enabled. |
| `/*` | Embedded web UI fallback when built with `web`. |

REST is direct-route-only: there is no `/v1/soma` action envelope. MCP remains one `soma` tool with an `action` argument.

## MCP Tool Reference

The runtime exposes one compact MCP tool, `soma`, with an `action` argument.
Built-in actions and dropped provider tools share that same dispatch path. This
keeps MCP discovery small while allowing the provider catalog to grow behind the
single tool.

<!-- BEGIN GENERATED README_ACTION_TABLE -->
<!-- Generated by scripts/generate-docs.py; do not edit by hand. -->
| Action | Scope | Cost | Transport | REST route | CLI | Parameters | Description |
|---|---|---|---|---|---|---|---|
| `greet` | `soma:read` | `cheap` | MCP + CLI + REST | `POST /v1/greet` | `soma greet [--name N]` | `name` (optional string) | Return a greeting. |
| `echo` | `soma:read` | `cheap` | MCP + CLI + REST | `POST /v1/echo` | `soma echo --message <msg>` | `message` (required string) | Echo a message back unchanged. |
| `status` | `soma:read` | `cheap` | MCP + CLI + REST | `GET /v1/status` | `soma status` | none | Return server status and configuration info. |
| `elicit_name` | `soma:read` | `cheap` | MCP-only | - | `_MCP-only_` | none | Ask the MCP client to collect a name, then return a personalised greeting. |
| `scaffold_intent` | `soma:read` | `moderate` | MCP-only | - | `_MCP-only_` | none | Collect scaffold setup intent through MCP elicitation and return JSON for the scaffold-project skill. |
| `help` | public | `cheap` | MCP + CLI + REST | `GET /v1/help` | `soma --help` | none | Show the action reference. |
<!-- END GENERATED README_ACTION_TABLE -->

Built-in business actions keep MCP + CLI + REST parity unless there is a
protocol reason they cannot. `elicit_name` and `scaffold_intent` are MCP-only
because they rely on MCP elicitation. `serve`, `mcp`, `doctor`, `watch`, `setup`,
and `package` are CLI operator commands, not business actions.

Dropped provider tools are MCP-enabled by default and REST-executable through
`POST /v1/tools/{action}` unless the tool explicitly sets
`rest.enabled=false`. A `rest` overlay can add a custom route, method, and
OpenAPI metadata; the generic route remains the web/adapter-safe execution
shape. CLI exposure is opt-in through each tool's `cli` overlay. Provider
prompts, resources, tasks, and elicitation forms are part of the provider
manifest contract and registry index; they are not mirrored to CLI or REST by
default.

## CLI Reference

The `soma` binary exposes operator commands and provider-backed actions through
the same registry snapshot used by MCP:

```bash
soma greet --name Alice
soma echo --message hello
soma status
soma help
soma providers validate
soma providers inspect
soma providers test status
soma providers list --dir ./examples/providers
soma providers lint --dir ./examples/providers
soma providers status --dir ./examples/providers
soma doctor
soma setup check
soma package generate --check
```

Provider tools opt in to CLI exposure with a `cli` overlay. Dynamic CLI flags
are derived from the provider input schema, so the generated provider catalogs
remain the source of truth for current action shapes.

## Safety And Trust Model

MCP callers never provide API keys, OAuth secrets, bearer tokens, passwords, or
other credentials in tool arguments. Credentials live in environment variables,
config files, appdata, or the upstream provider runtime.

Provider manifests are validated before dispatch. The registry enforces surface
opt-ins, JSON Schema input validation, auth scope, declared host capabilities,
destructive-action confirmation, response-size limits, and structured provider
errors. Python, LangChain, LlamaIndex, and TypeScript provider files are trusted
local code; WASM providers run through the sandboxed WASM provider path; OpenAPI
and MCP providers delegate trust to their configured upstream service.

## Authentication

The HTTP server supports four auth policies:

| Policy | When | Effect |
|---|---|---|
| Loopback development | Loopback bind, or `SOMA_MCP_NO_AUTH=true` on loopback | No auth middleware, no scope checks. |
| Bearer token | `SOMA_MCP_TOKEN` set | `/mcp` and `/v1/*` require `Authorization: Bearer <token>`. |
| OAuth | `SOMA_MCP_AUTH_MODE=oauth` with at least one configured provider | Browser-based Google, Authelia, or GitHub login issues JWT bearer tokens. |
| Trusted gateway | `SOMA_NOAUTH=true` on non-loopback | Local auth and scope checks disabled because an upstream gateway is responsible. |

The startup guard refuses non-loopback unauthenticated binds unless bearer,
OAuth, or trusted-gateway mode is configured. `/health`, `/readyz`, `/status`,
and `/openapi.json` are public by design and return only safe runtime metadata.

See [docs/AUTH.md](docs/AUTH.md) for the detailed auth model.

## Configuration

Values load from `config.toml`, local appdata files, and environment variables;
explicit environment variables win. The built-in offline provider works without
real credentials, but generated projects should mark their real
upstream/platform credentials as required.

| Variable | Required | Default | Description |
|---|---|---|---|
| `SOMA_API_URL` | no | empty | Deployed platform API or upstream service URL. Empty selects stub/offline behavior. |
| `SOMA_API_KEY` | no | empty | Bearer token or upstream service API key. |
| `SOMA_PROVIDER_DIR` | no | `providers` | Directory scanned for drop-in provider files. Relative paths resolve from the current working directory. |
| `SOMA_MCP_HOST` | no | `127.0.0.1` | HTTP server bind host. |
| `SOMA_MCP_PORT` | no | `40060` | HTTP server bind port. |
| `SOMA_MCP_SERVER_NAME` | no | `soma` | MCP server name advertised to clients. |
| `SOMA_MCP_NO_AUTH` | no | `false` | Disable auth for loopback development. |
| `SOMA_NOAUTH` | no | `false` | Trusted-gateway non-loopback no-auth mode. |
| `SOMA_MCP_TOKEN` | bearer | empty | Static bearer token. |
| `SOMA_MCP_ALLOWED_HOSTS` | no | empty | Extra comma-separated Host header values. |
| `SOMA_MCP_ALLOWED_ORIGINS` | no | empty | Extra comma-separated CORS origins. |
| `SOMA_MCP_TRACE_HEADERS` | no | `off` | Trusted inbound HTTP trace extraction: `off`, `trusted`, or `trusted-with-baggage`. |
| `SOMA_MCP_AUTH_MODE` | no | `bearer` | `bearer` or `oauth`. |
| `SOMA_MCP_PUBLIC_URL` | OAuth | empty | Public URL for OAuth metadata and callbacks. |
| `SOMA_MCP_GOOGLE_CLIENT_ID` | OAuth | empty | Google OAuth client ID. |
| `SOMA_MCP_GOOGLE_CLIENT_SECRET` | OAuth | empty | Google OAuth client secret. |
| `SOMA_MCP_AUTHELIA_ISSUER_URL` | Authelia | empty | HTTPS Authelia OIDC issuer URL. |
| `SOMA_MCP_AUTHELIA_CLIENT_ID` | Authelia | empty | Authelia OIDC client ID. |
| `SOMA_MCP_AUTHELIA_CLIENT_SECRET` | Authelia | empty | Authelia OIDC client secret. |
| `SOMA_MCP_GITHUB_CLIENT_ID` | GitHub | empty | GitHub OAuth App client ID. |
| `SOMA_MCP_GITHUB_CLIENT_SECRET` | GitHub | empty | GitHub OAuth App client secret. |
| `SOMA_MCP_AUTH_DEFAULT_PROVIDER` | no | first configured | Provider used when a request omits `provider`; automatic priority is Google, Authelia, GitHub. |
| `SOMA_MCP_AUTH_ADMIN_EMAIL` | OAuth | empty | Initial/admin OAuth email. |
| `RUST_LOG` | no | `info` | Log filter. Stdio mode suppresses noisy logs to avoid corrupting JSON-RPC. |

Keep `SOMA_MCP_TRACE_HEADERS=off` unless the server is bound to loopback or a
trusted gateway strips or overwrites trace headers from untrusted clients.
Bearer/OAuth authentication alone is not that trust boundary. See
[docs/TRACE_CONTEXT.md](docs/TRACE_CONTEXT.md) for the complete inbound-only
trace-header contract.

Samples:

- [.env.example](.env.example) for secrets, URLs, and runtime env.
- [config.soma.toml](config.soma.toml) for non-secret defaults.

Provider callback paths default to `/auth/google/callback`,
`/auth/authelia/callback`, and `/auth/github/callback`; callback and scope
overrides are listed in `docs/ENV.md` in the source repository. GitHub OAuth
Apps do not provide an upstream refresh token, so GitHub-authenticated sessions
do not receive a local refresh token and must sign in again after their access
token expires. See `docs/AUTH.md` in the source repository for provider
selection and security details.

## Development

```bash
# Build profiles
cargo build --bin soma --no-default-features --features local-adapter
cargo build --bin soma --no-default-features --features server
cargo build --bin soma --features full

# Run checks
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo nextest run
cargo xtask contract-audit
cargo xtask generate-provider-surfaces --check

# Common just recipes
just dev                 # loopback HTTP server with local no-auth
just mcp                 # stdio MCP
just greet               # CLI smoke test
just doctor              # pre-flight check
just build-local         # local adapter binary
just build-full          # web assets + full platform binary
just verify              # fmt, lint, check, test
just check-docs          # generated docs/metadata current
just scaffold-contract-check
just validate-plugin
```

`cargo xtask ci` runs the main local CI sequence. Optional tools such as
`cargo-nextest`, `taplo`, and `cargo-audit` are used when installed.

## Client Configuration

Streamable HTTP:

```json
{
  "mcpServers": {
    "soma": {
      "url": "http://localhost:40060/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_TOKEN"
      }
    }
  }
}
```

Stdio:

```json
{
  "mcpServers": {
    "soma": {
      "command": "/path/to/soma",
      "args": ["mcp"],
      "env": {
        "SOMA_API_URL": "https://api.example.com",
        "SOMA_API_KEY": "YOUR_API_KEY",
        "RUST_LOG": "warn"
      }
    }
  }
}
```

For generated projects, replace `soma`, `SOMA_*`, tool names, scopes,
and paths with the generated service names.

## Plugin Surfaces

The repo ships one shared Soma plugin package under [plugins/soma](plugins/soma)
for Claude Code, Codex, and Gemini surfaces. Plugin manifests are versionless;
release tooling derives version identity from git state. The plugin package can
use the local stdio adapter and includes setup/doctor support for appdata and
environment files.

Primary docs:

- [docs/PLUGINS.md](docs/PLUGINS.md)
- [plugins/soma/.codex-plugin/README.md](plugins/soma/.codex-plugin/README.md)
- [plugins/soma/skills/soma/SKILL.md](plugins/soma/skills/soma/SKILL.md)
- [plugins/soma/skills/scaffold-project/SKILL.md](plugins/soma/skills/scaffold-project/SKILL.md)

## Distribution Contract

The `soma` release component is defined in
[release/components.toml](release/components.toml). Version-bearing artifacts
must stay aligned across the Rust package, `Cargo.lock`, `server.json`, the npm
package, generated OpenAPI metadata, OCI image identifiers, and the changelog.

Plugin manifests stay versionless. Marketplace and plugin release identity is
derived from git/package metadata, while `server.json` and generated provider
surface docs describe the currently shipped runtime surface. Run
`cargo xtask check-version-sync`, `cargo xtask generate-provider-surfaces --check`,
and `cargo xtask check-docs` before publishing release metadata.

## Web UI

The `web` feature serves the static export bundled by `soma-web`. Editable
frontend source lives in [apps/web](apps/web), and `cargo xtask sync-web-source`
copies that source into the Rust crate bundle.

Useful commands:

```bash
cargo xtask build-web
cargo xtask sync-web-source
cargo xtask check-web-source-sync
pnpm -C apps/web validate
```

Generated projects that do not need a human UI should use `local-adapter`,
`server`, or a custom feature set without `web`.

## Deployment

The full platform profile is designed for one deployable `soma` binary. The repository
also includes Docker and Compose samples:

- [config/Dockerfile](config/Dockerfile)
- [docker-compose.prod.yml](docker-compose.prod.yml)
- [entrypoint.sh](entrypoint.sh)

When adapting a generated project, verify the canonical binary name, exposed
port, healthcheck port, image labels, service user/group, data volume, and
required environment variables. The scaffold verifier catches several
scaffold-only artifacts, but deployment files still need service-specific review
before publishing an image.

## When Drop-In Providers Are Not Enough

Most new capabilities should start as provider files. Reach for native Rust or a
scaffolded product when you need a reusable crate boundary, richer service state,
custom clients, background jobs, a dedicated package identity, or hand-tuned
transport behavior.

For a generated product, start by printing the profile-aware checklist:

```bash
cargo xtask scaffold --adapt-plan ../generated/myservice-mcp
```

Then generate reviewable starter artifacts for the repetitive action wiring:

```bash
cargo xtask scaffold \
  --write-action-starters ../generated/myservice-mcp \
  --actions actions.json
```

This writes `docs/action-starters/` in the generated project with snippets for
action metadata, MCP dispatch, CLI variants, service stubs, and test coverage.

1. Replace the stub client in `crates/soma/client/src/client.rs` only when the provider file path is not enough.
2. Put domain logic in `crates/soma/application/src/service.rs` or focused service modules.
3. Register native provider/action metadata so MCP, CLI, REST, docs, and plugins stay registry-driven.
4. Regenerate MCP schema docs, provider surface docs, and OpenAPI so generated surfaces reflect the provider registry.
5. Add REST handlers only for infrastructure routes; business actions should stay registry-backed direct routes.
6. Update config fields and env prefixes in `crates/soma/config/src/config.rs`.
7. Update `.env.example`, `config.soma.toml`, plugin options, and setup mappings.
8. Update `server.json`, plugin metadata, repository URLs, Docker labels, and release metadata.
9. Add tests for MCP dispatch, CLI parsing, REST routes, provider loading, and service behavior.
10. For generated/exported projects, run scaffold verification plus the
    project's local quality gates.

For public repositories, also review tracked docs, generated metadata, CI runner
configuration, and secret-scanning allowlists before publishing.

## Troubleshooting

- `soma doctor` checks local configuration, appdata, and connectivity.
- `soma providers validate` confirms provider manifests and compiled schemas
  against the *loaded, live* registry.
- `soma providers inspect` shows provider surfaces, capability posture, and
  generated action inventory.
- `soma providers list|lint|status` inspect drop-in provider files on disk
  without loading the registry or executing any handler — safe to run before
  the runtime touches TS/WASM/MCP/OpenAPI providers. See
  [`docs/PROVIDERS.md`](docs/PROVIDERS.md).
- Stdio mode keeps logs quiet so JSON-RPC is not corrupted; use HTTP mode or
  file logs when investigating noisy startup failures.
- If generated docs drift, run `cargo xtask generate-provider-surfaces --write`
  and then re-run the `--check` command.

## Related Servers

- [unifi-rmcp](https://github.com/jmagar/unifi-rmcp) - UniFi controller REST API bridge.
- [tailscale-rmcp](https://github.com/jmagar/tailscale-rmcp) - Tailscale API bridge for devices, users, and tailnet operations.
- [unraid-rmcp](https://github.com/jmagar/unraid-rmcp) - Unraid GraphQL bridge for NAS and server management.
- [apprise-rmcp](https://github.com/jmagar/apprise-rmcp) - Apprise notification fan-out bridge for many delivery backends.
- [gotify-rmcp](https://github.com/jmagar/gotify-rmcp) - Gotify push notification bridge for sends, messages, apps, and clients.
- [arcane-rmcp](https://github.com/jmagar/arcane-rmcp) - Arcane Docker management bridge for containers and related resources.
- [yarr](https://github.com/jmagar/yarr) - Media-stack bridge for Sonarr, Radarr, Prowlarr, Plex, and related services.
- [ytdl-rmcp](https://github.com/jmagar/ytdl-rmcp) - Media download and metadata workflow server.
- [synapse-rmcp](https://github.com/jmagar/synapse-rmcp) - Local Synapse workflow server for scout and flux actions.
- [cortex](https://github.com/jmagar/cortex) - Syslog and homelab log aggregation MCP server.
- [axon](https://github.com/jmagar/axon) - RAG, crawl, scrape, extract, and semantic search project.
- [labby](https://github.com/jmagar/labby) - Homelab control plane and MCP gateway project.
- [lumen](https://github.com/jmagar/lumen) - Local semantic code search MCP server.

## Documentation

| Topic | Docs |
|---|---|
| Architecture and layering | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/PATTERNS.md](docs/PATTERNS.md) |
| Dynamic provider runtime | [docs/specs/dynamic-provider-runtime.md](docs/specs/dynamic-provider-runtime.md), [docs/generated/provider-surfaces.md](docs/generated/provider-surfaces.md) |
| Provider manifest contract | [docs/contracts/provider-manifest.schema.json](docs/contracts/provider-manifest.schema.json), [docs/contracts/examples/provider-manifests](docs/contracts/examples/provider-manifests) |
| Scaffold workflow | [docs/SCAFFOLD.md](docs/SCAFFOLD.md), [docs/CARGO_GENERATE.md](docs/CARGO_GENERATE.md) |
| Scaffold intent contract | [docs/specs/scaffold-intent-handoff.md](docs/specs/scaffold-intent-handoff.md), [docs/contracts/scaffold-intent.schema.json](docs/contracts/scaffold-intent.schema.json) |
| MCP action schema | [docs/MCP_SCHEMA.md](docs/MCP_SCHEMA.md) |
| REST OpenAPI | [docs/generated/openapi.json](docs/generated/openapi.json) |
| Auth | [docs/AUTH.md](docs/AUTH.md) |
| Plugins | [docs/PLUGINS.md](docs/PLUGINS.md) |
| Release/versioning | [release/components.toml](release/components.toml), [docs/MCP-REGISTRY-PUBLISH-GUIDE.md](docs/MCP-REGISTRY-PUBLISH-GUIDE.md) |
| Automation | [xtask/README.md](xtask/README.md), [scripts/README.md](scripts/README.md) |
| Tests | [apps/soma/tests/README.md](apps/soma/tests/README.md) |

## Verification

Product runtime gates:

```bash
cargo xtask check-docs
cargo xtask generate-provider-surfaces --check
cargo xtask check-schema-docs --check
cargo xtask check-openapi --check
cargo xtask validate-plugin-layout
cargo xtask check-version-sync
just verify
```

Scaffold/template gates are a separate lane. Run them when a change touches the
scaffold contract, cargo-generate post-processing, or generated-project output:

```bash
cargo xtask check-scaffold-intent-contract
cargo xtask scaffold --verify ../generated/myservice-mcp
cargo xtask check-cargo-generate
```

Use targeted checks while iterating, then run the broader product and affected
scaffold gates before release.

## License

MIT
