# rmcp-template

`rmcp-template` is a batteries-included RMCP server runtime and shipping binary
for bringing new agent capabilities online with as little custom Rust as
possible. It locks in the production patterns that every server in the family
keeps rediscovering: one compact MCP tool, stdio and Streamable HTTP transports,
CLI parity, direct REST routes, auth/OAuth, observability, plugin packaging,
web fallback, Docker/runtime templates, generated contracts, and release
automation.

The repository can still scaffold a renamed project, but it is no longer just a
template. The default product path is to run `rtemplate` or `rtemplate-server`,
drop provider files into `providers/` (or point `RTEMPLATE_PROVIDER_DIR`
elsewhere), and let the provider registry project those capabilities across MCP,
CLI, REST, OpenAPI, Palette summaries, generated docs, and plugin metadata.
Provider manifests also carry MCP-native prompt, resource, task, and elicitation
metadata for the registry contract. Scaffolding is the path for creating a new
distributable repo with the same locked-in runtime.

## Bring A Server Online

| Path | Use when | You author | Runtime supplies |
|---|---|---|---|
| Drop-in provider | You can describe a capability as a manifest, script, WASM module, OpenAPI operation, or upstream MCP call. | Files under `providers/` with tools, prompts, resources, env needs, capability grants, and surface overlays. | MCP tool dispatch, dynamic CLI commands, direct REST routes, schema validation, auth policy, refresh, OpenAPI/Palette summaries, generated docs, and plugin metadata. |
| Static Rust provider | The capability needs native Rust, tight integration, or reusable crates. | A Rust provider/action registered with the provider registry. | The same MCP/CLI/REST/docs/plugin projection without per-surface rewrites. |
| Scaffolded product | You need a renamed repository, package identity, ports, plugins, Docker labels, and release metadata. | A `scaffold_intent` payload or `cargo xtask scaffold` options. | A compiling product repo, scaffold report, cargo-generate post-processing, and verification checks. |
| Custom profile | You need a narrower binary or deployment shape. | Cargo feature selection. | The same runtime crates behind `local-adapter`, `server`, and `full` profiles. |

## Batteries Included

- One compact MCP service tool (`example`) with `action` dispatch, so agent tool
  lists stay small even as provider catalogs grow.
- Two binaries: `rtemplate` for local CLI + stdio MCP, and `rtemplate-server`
  for REST API + Streamable HTTP MCP + stdio MCP + optional web UI.
- Dynamic provider loading from `.json`, `.ts`, `.py`, and `.wasm` files, plus
  native Rust providers and upstream MCP/OpenAPI provider kinds.
- Provider manifest contracts for tools, prompts, resources, tasks,
  elicitation forms, env requirements, capability grants, and surface overlays.
- Shared validation, destructive-action confirmation, auth/scope enforcement,
  response limits, redaction, logging, metrics, generated OpenAPI, generated
  provider surface docs, plugin manifests, setup, doctor, and release tooling.

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
| `oauth` | Google OAuth and JWT issuance on top of `auth`. |
| `mcp-http` | Streamable HTTP MCP mounted in Axum. |
| `web` | Embedded static web UI fallback. |
| `observability` | Metrics/tracing hooks. |
| `plugin` | Plugin setup/support helpers. |
| `local-adapter` | Lean local binary: `cli` + `mcp-stdio`. |
| `server` | Deployable server binary: `cli` + `api` + HTTP MCP + stdio MCP. |
| `full` | Complete platform profile: local adapter, server, web, OAuth, observability, and plugin support. |

## Quickstart

Run the product as-is:

```bash
git clone https://github.com/jmagar/rmcp-template
cd rmcp-template

# Full server binary: REST API + HTTP MCP + web fallback on :40060
cargo run --bin rtemplate-server -- serve mcp

# Local binary: stdio MCP
cargo run --bin rtemplate -- mcp

# Local binary: CLI
cargo run --bin rtemplate -- greet --name Alice
```

Useful smoke checks:

```bash
curl http://localhost:40060/health
cargo run --bin rtemplate -- status
cargo run --bin rtemplate -- doctor
```

Call the MCP endpoint directly:

```bash
curl -s -X POST http://localhost:40060/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"example","arguments":{"action":"greet","name":"Alice"}}}'
```

## Drop In A Provider

The fastest path for a new server is provider-first. Add a provider manifest or
module to `providers/`, then run the same binary. Use `RTEMPLATE_PROVIDER_DIR`
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
cargo run --bin rtemplate -- hello-local --name Alice
```

Run the server and call the same provider over REST and MCP:

```bash
cargo run --bin rtemplate-server -- serve mcp

curl -s -X POST http://localhost:40060/v1/hello-local \
  -H "Content-Type: application/json" \
  -d '{"name":"Alice"}'

curl -s -X POST http://localhost:40060/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"example","arguments":{"action":"hello_local","name":"Alice"}}}'
```

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
`langchain`, and `llamaindex`. See
[docs/specs/dynamic-provider-runtime.md](docs/specs/dynamic-provider-runtime.md),
[docs/contracts/provider-manifest.schema.json](docs/contracts/provider-manifest.schema.json),
and [docs/generated/provider-surfaces.md](docs/generated/provider-surfaces.md).

## Scaffold A New Project

Use `cargo xtask scaffold` when the provider-first path needs to become a new
repository with its own crate names, binary names, ports, plugin package,
Docker metadata, release metadata, and docs. It can plan without touching files,
generate with `cargo-generate` plus the Rust post-processor, write
`docs/scaffold-report.md`, and verify the generated project.

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
  crates/rtemplate-service/src/provider_registry.rs
  Validates provider manifests, computes snapshots/fingerprints, indexes tools,
  prompts, resources, CLI commands, REST routes, and MCP primitives.

Provider sources
  crates/rtemplate-service/src/providers/
  Static Rust, file-backed JSON manifests, TypeScript AI SDK sidecars, Python
  LangChain/LlamaIndex sidecars, WASM, OpenAPI-backed providers, and upstream
  MCP providers.

ExampleService
  crates/rtemplate-service/src/app.rs
  Built-in product/service logic used by the static Rust provider.

Transport shims
  crates/rtemplate-cli/src/lib.rs        CLI parser and output formatting.
  crates/rtemplate-mcp/src/tools.rs      MCP JSON args to service calls.
  crates/rtemplate-api/src/api.rs        REST extractors to service calls.
  crates/rmcp-template/src/routes.rs     Axum router, auth, MCP, API, web fallback.

Built-in action metadata
  crates/rtemplate-contracts/src/actions.rs
  Native action metadata, validation, cached catalog/help, and native dispatch.
```

The thin-shim rule is strict:

1. Parse input at the surface.
2. Call the provider registry or service runtime.
3. Return or print the result.

Do not put business rules in CLI, MCP, REST handlers, or `main.rs`.

## Runtime Surfaces

The full server binary can run the whole app from one executable:

```bash
rtemplate-server serve mcp   # HTTP server: REST API + Streamable HTTP MCP + web fallback
rtemplate-server mcp         # stdio MCP transport
rtemplate-server status      # CLI command through the server binary
```

The local adapter binary is optimized for plugin/local use:

```bash
rtemplate mcp                # stdio MCP transport
rtemplate greet --name Alice # CLI command
rtemplate doctor             # operator pre-flight checks
rtemplate watch              # poll /health and emit state changes
rtemplate setup check        # plugin/appdata setup checks
```

Both binaries load the provider registry. File providers default to
`./providers` and can be moved with `RTEMPLATE_PROVIDER_DIR`. CLI startup,
MCP dispatch, and dynamic REST routes refresh file providers before execution,
then enforce the active provider snapshot's schema, surface, scope, capability,
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
| `/v1/{provider-route}` | Dynamic provider REST routes when a provider tool opts in with a REST overlay. |
| `/mcp/.well-known/*` | OAuth metadata when OAuth is enabled. |
| `/*` | Embedded web UI fallback when built with `web`. |

REST is direct-route-only: there is no `/v1/example` action envelope. MCP remains one `example` tool with an `action` argument.

## MCP Tool Actions

The runtime exposes one compact MCP tool, `example`, with an `action` argument.
Built-in actions and dropped provider tools share that same dispatch path. This
keeps MCP discovery small while allowing the provider catalog to grow behind the
single tool.

<!-- BEGIN GENERATED README_ACTION_TABLE -->
<!-- Generated by scripts/generate-docs.py; do not edit by hand. -->
| Action | Scope | Cost | Transport | REST route | CLI | Parameters | Description |
|---|---|---|---|---|---|---|---|
| `greet` | `example:read` | `cheap` | MCP + CLI + REST | `POST /v1/greet` | `rtemplate greet [--name N]` | `name` (optional string) | Return a greeting. |
| `echo` | `example:read` | `cheap` | MCP + CLI + REST | `POST /v1/echo` | `rtemplate echo --message <msg>` | `message` (required string) | Echo a message back unchanged. |
| `status` | `example:read` | `cheap` | MCP + CLI + REST | `GET /v1/status` | `rtemplate status` | none | Return server status and configuration info. |
| `help` | public | `cheap` | MCP + CLI + REST | `GET /v1/help` | `rtemplate --help` | none | Show the action reference. |
| `elicit_name` | `example:read` | `cheap` | MCP-only | - | `_MCP-only_` | none | Ask the MCP client to collect a name, then return a personalised greeting. |
| `scaffold_intent` | `example:read` | `moderate` | MCP-only | - | `_MCP-only_` | none | Collect scaffold setup intent through MCP elicitation and return JSON for the scaffold-project skill. |
<!-- END GENERATED README_ACTION_TABLE -->

Built-in business actions keep MCP + CLI + REST parity unless there is a
protocol reason they cannot. `elicit_name` and `scaffold_intent` are MCP-only
because they rely on MCP elicitation. `serve`, `mcp`, `doctor`, `watch`, `setup`,
and `package` are CLI operator commands, not business actions.

Dropped provider tools are MCP-enabled by default. CLI and REST exposure are
opt-in through each tool's `cli` and `rest` overlays. Provider prompts,
resources, tasks, and elicitation forms are part of the provider manifest
contract and registry index; they are not mirrored to CLI or REST by default.

## Authentication

The HTTP server supports four auth policies:

| Policy | When | Effect |
|---|---|---|
| Loopback development | Loopback bind, or `RTEMPLATE_MCP_NO_AUTH=true` on loopback | No auth middleware, no scope checks. |
| Bearer token | `RTEMPLATE_MCP_TOKEN` set | `/mcp` and `/v1/*` require `Authorization: Bearer <token>`. |
| OAuth | `RTEMPLATE_MCP_AUTH_MODE=oauth` with Google OAuth settings | Browser-based Google OAuth issues JWT bearer tokens. |
| Trusted gateway | `RTEMPLATE_NOAUTH=true` on non-loopback | Local auth and scope checks disabled because an upstream gateway is responsible. |

The startup guard refuses non-loopback unauthenticated binds unless bearer,
OAuth, or trusted-gateway mode is configured. `/health`, `/readyz`, `/status`,
and `/openapi.json` are public by design and return only safe runtime metadata.

See [docs/AUTH.md](docs/AUTH.md) for the detailed auth model.

## Configuration

Values load from `config.toml`, local appdata files, and environment variables;
explicit environment variables win. The template stub works without real
credentials, but generated projects should mark their real upstream/platform
credentials as required.

| Variable | Required | Default | Description |
|---|---|---|---|
| `RTEMPLATE_API_URL` | no | empty | Deployed platform API or upstream service URL. Empty selects stub/offline behavior. |
| `RTEMPLATE_API_KEY` | no | empty | Bearer token or upstream service API key. |
| `RTEMPLATE_PROVIDER_DIR` | no | `providers` | Directory scanned for drop-in provider files. Relative paths resolve from the current working directory. |
| `RTEMPLATE_MCP_HOST` | no | `127.0.0.1` | HTTP server bind host. |
| `RTEMPLATE_MCP_PORT` | no | `40060` | HTTP server bind port. |
| `RTEMPLATE_MCP_SERVER_NAME` | no | `template-rmcp` | MCP server name advertised to clients. |
| `RTEMPLATE_MCP_NO_AUTH` | no | `false` | Disable auth for loopback development. |
| `RTEMPLATE_NOAUTH` | no | `false` | Trusted-gateway non-loopback no-auth mode. |
| `RTEMPLATE_MCP_TOKEN` | bearer | empty | Static bearer token. |
| `RTEMPLATE_MCP_ALLOWED_HOSTS` | no | empty | Extra comma-separated Host header values. |
| `RTEMPLATE_MCP_ALLOWED_ORIGINS` | no | empty | Extra comma-separated CORS origins. |
| `RTEMPLATE_MCP_AUTH_MODE` | no | `bearer` | `bearer` or `oauth`. |
| `RTEMPLATE_MCP_PUBLIC_URL` | OAuth | empty | Public URL for OAuth metadata and callbacks. |
| `RTEMPLATE_MCP_GOOGLE_CLIENT_ID` | OAuth | empty | Google OAuth client ID. |
| `RTEMPLATE_MCP_GOOGLE_CLIENT_SECRET` | OAuth | empty | Google OAuth client secret. |
| `RTEMPLATE_MCP_AUTH_ADMIN_EMAIL` | OAuth | empty | Initial/admin OAuth email. |
| `RUST_LOG` | no | `info` | Log filter. Stdio mode suppresses noisy logs to avoid corrupting JSON-RPC. |

Templates:

- [.env.example](.env.example) for secrets, URLs, and runtime env.
- [config.example.toml](config.example.toml) for non-secret defaults.

## Development Commands

```bash
# Build profiles
cargo build --bin rtemplate --no-default-features --features local-adapter
cargo build --bin rtemplate-server --no-default-features --features server
cargo build --bin rtemplate-server --features full

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
just build-full          # web assets + full server binary
just verify              # fmt, lint, check, test
just check-docs          # generated docs/metadata current
just scaffold-contract-check
just validate-plugin
```

`cargo xtask ci` runs the main local CI sequence. Optional tools such as
`cargo-nextest`, `taplo`, and `cargo-audit` are used when installed.

## MCP Client Configuration

Streamable HTTP:

```json
{
  "mcpServers": {
    "rtemplate": {
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
    "rtemplate": {
      "command": "/path/to/rtemplate",
      "args": ["mcp"],
      "env": {
        "RTEMPLATE_API_URL": "https://api.example.com/v1",
        "RTEMPLATE_API_KEY": "YOUR_API_KEY",
        "RUST_LOG": "warn"
      }
    }
  }
}
```

For generated projects, replace `rtemplate`, `RTEMPLATE_*`, tool names, scopes,
and paths with the generated service names.

## Plugin Surfaces

The repo ships one shared plugin package under [plugins/rtemplate](plugins/rtemplate)
for Claude Code, Codex, and Gemini surfaces. Plugin manifests are versionless;
release tooling derives version identity from git state. The plugin package can
use the local stdio adapter and includes setup/doctor support for appdata and
environment files.

Primary docs:

- [docs/PLUGINS.md](docs/PLUGINS.md)
- [plugins/rtemplate/.codex-plugin/README.md](plugins/rtemplate/.codex-plugin/README.md)
- [plugins/rtemplate/skills/rtemplate/SKILL.md](plugins/rtemplate/skills/rtemplate/SKILL.md)
- [plugins/rtemplate/skills/scaffold-project/SKILL.md](plugins/rtemplate/skills/scaffold-project/SKILL.md)

## Web UI

The `web` feature serves the static export bundled by `rtemplate-web`. Editable
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

## Deployment Templates

The full server profile is designed for one deployable binary. The repository
also includes Docker and Compose templates:

- [config/Dockerfile](config/Dockerfile)
- [docker-compose.prod.yml](docker-compose.prod.yml)
- [entrypoint.sh](entrypoint.sh)

When adapting a generated project, verify the server binary name, exposed port,
healthcheck port, image labels, service user/group, data volume, and required
environment variables. The scaffold verifier catches several template-only
artifacts, but deployment files still need service-specific review before
publishing an image.

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

1. Replace the stub client in `crates/rtemplate-service/src/example.rs` only when the provider file path is not enough.
2. Put domain logic in `crates/rtemplate-service/src/app.rs` or focused service modules.
3. Register native provider/action metadata so MCP, CLI, REST, docs, and plugins stay registry-driven.
4. Regenerate MCP schema docs, provider surface docs, and OpenAPI so generated surfaces reflect the provider registry.
5. Add REST handlers only for infrastructure routes; business actions should stay registry-backed direct routes.
6. Update config fields and env prefixes in `crates/rtemplate-contracts/src/config.rs`.
7. Update `.env.example`, `config.example.toml`, plugin options, and setup mappings.
8. Update `server.json`, plugin metadata, repository URLs, Docker labels, and release metadata.
9. Add tests for MCP dispatch, CLI parsing, REST routes, provider loading, and service behavior.
10. Run scaffold verification and the local quality gates.

For public repositories, also review tracked docs, generated metadata, CI runner
configuration, and secret-scanning allowlists before publishing.

## Documentation Map

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
| Tests | [crates/rmcp-template/tests/README.md](crates/rmcp-template/tests/README.md) |

## Verification

Common local gates:

```bash
cargo xtask scaffold --verify ../generated/myservice-mcp
cargo xtask check-docs
cargo xtask generate-provider-surfaces --check
cargo xtask check-schema-docs --check
cargo xtask check-openapi --check
cargo xtask check-scaffold-intent-contract
cargo xtask validate-plugin-layout
cargo xtask check-version-sync
just verify
```

Use targeted checks while iterating, then run the broader gates before release.

## License

MIT
