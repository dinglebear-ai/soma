---
title: "RMCP README Guide"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: false
upstream_refs:
  - "README.md"
  - "docs/PATTERNS.md"
  - "docs/SCAFFOLD.md"
last_reviewed: "2026-07-11"
---

# RMCP README Guide

Use this when creating or refreshing a top-level `README.md` for a Rust MCP
server built with the `rmcp` crate. The root README should be the public entry
point: quick to scan, accurate about runtime surfaces, and linked to generated
contracts for details that are easy to drift.

Soma was extracted from the current top-level READMEs for:
`unifi-rmcp`, `tailscale-rmcp`, `unraid-rmcp`, `apprise-rmcp`,
`gotify-rmcp`, `arcane-rmcp`, `yarr-rmcp`, `ytdl-mcp`, `synapse`,
`cortex`, `axon`, `lab`, `agentcast`, and `soma`.

## Applicability

Use this guide for Rust MCP/RMCP server repositories that expose an MCP server,
CLI, and deployable binary. It is not a universal README template.

Apply it directly to focused upstream-client servers such as `gotify-rmcp`,
`tailscale-rmcp`, `unifi-rmcp`, `unraid-rmcp`, `apprise-rmcp`,
`arcane-rmcp`, and similar repositories. For broader platform repos, generated
provider repos, or multi-surface applications, apply the invariants rather than
forcing every heading into place.

Do not force this exact shape onto design systems, web apps, infra-only repos,
Go/Python CLIs, docs-only repos, or homelab service runbooks. If a repo is not
an RMCP server, use a README structure native to that product.

## Exceptions

Exceptions are expected when product identity is broader than `<service>-rmcp`
or a repo ships multiple first-class surfaces. Document the exception near the
first naming or surfaces table instead of hiding it.

Examples:

- `cortex` keeps the repo and CLI name because it is a log platform, not only an
  MCP wrapper.
- `axon` and `lab` are platform repos with REST, web, runtime, and local
  service contracts that should not be flattened into a small upstream-client
  README.
- `yarr-rmcp` covers a family surface, so generated operation docs and product
  boundary language matter more than a tiny action list.

Even when a README needs an exception, keep these guide invariants: first safe
CLI and MCP calls, credential boundaries, generated-doc ownership, destructive
action gates, runtime-surface honesty, and distribution/version contracts.

## Source Patterns

| Source README | Pattern to reuse |
|---|---|
| [unifi-rmcp](https://github.com/jmagar/unifi-rmcp) | Explain upstream API families before listing actions; separate official/generated/internal/hybrid action groups; include live-smoke guidance. |
| [tailscale-rmcp](https://github.com/jmagar/tailscale-rmcp) | Teach one required domain concept early; include a clear naming table, transport table, auth modes, and destructive interlock. |
| [unraid-rmcp](https://github.com/jmagar/unraid-rmcp) | Use an architecture diagram, first raw MCP call, transport table, action parameters, pagination behavior, and config precedence. |
| [apprise-rmcp](https://github.com/jmagar/apprise-rmcp) | Keep a compact "what it does" section and show simple tool-call examples before deeper reference material. |
| [gotify-rmcp](https://github.com/jmagar/gotify-rmcp) | Document upstream token types, raw JSON-RPC examples, CLI parity, destructive safety, and HTTP endpoint auth. |
| [arcane-rmcp](https://github.com/jmagar/arcane-rmcp) | Add a `Surfaces` table, group action/subaction domains, and state that credentials are config/env only. |
| [yarr](https://github.com/jmagar/yarr) | Include a product boundary, install/distribution matrix, generated-vs-curated operation split, tool-mode explanation, and credential-rejection rules. |
| [ytdl-rmcp](https://github.com/jmagar/ytdl-rmcp) | Lead with capabilities, distribution forms, self-contained runtime behavior, detailed env table, bootstrap trust model, and "how it works" steps. |
| [synapse-rmcp](https://github.com/jmagar/synapse-rmcp) | State porting/parity claims directly, list known gaps, and document multiple MCP tools without hiding REST/web status. |
| [cortex](https://github.com/jmagar/cortex) | Link full schema docs for large action surfaces; document prompts/resources/MCP Apps; include security, storage, deployment, and verification sections. |
| [axon](https://github.com/jmagar/axon) | Separate current production contract from future pipeline goals; document setup flow, Docker stack, config home, notable capabilities, and troubleshooting. |
| [labby](https://github.com/jmagar/labby) | Treat topic docs and generated catalogs as authoritative; include a contents map and do not hand-maintain generated action/env inventories in the README. |
| [agentcast](https://github.com/jmagar/agentcast) | Separate current implementation from target contract; document product boundaries and trust assumptions for untrusted MCP metadata. |
| [soma](https://github.com/jmagar/soma) | Include scaffold profiles, generated action table, auth policies, adaptation checklist, docs map, and verification gates. |

## Golden Example

[gotify-rmcp](https://github.com/jmagar/gotify-rmcp) is the first focused
upstream-client pilot for this guide. Use its root README as the minimal
filled-in example when aligning the rest of the RMCP family: compact domain
intro, explicit token model, install matrix, safe CLI/MCP calls, client
configuration, curated-vs-generated docs split, destructive confirmation, and
distribution contract.

Do not treat the pilot as the only valid shape. It is a small Gotify bridge, so
larger repos should preserve their own product boundaries while keeping the
same operator experience goals.

## README Rules

- The first screen should answer: what this server does, what it connects to,
  which MCP tool(s) it exposes, and how to run it.
- Include a 30-second value prop before detailed reference material: the
  upstream/local capability, the agent-facing outcome, the tool name, and the
  fastest safe command.
- State the product boundary. A good "Not for" section prevents misuse better
  than a long feature list.
- Prefer "capabilities and boundaries" language over generic feature lists:
  name what the repo owns, what it delegates to the upstream service, and what
  it deliberately refuses to do.
- Show installation and first successful call before exhaustive reference.
- Include one CLI call and one MCP call that are safe, read-only, and expected
  to work for a correctly configured install.
- Keep action tables complete enough for a human scan; move detailed generated
  schemas to `docs/MCP_SCHEMA.md` or another generated contract.
- Make the README curated, not generated. Generated action, env, route, schema,
  or provider catalogs are source of truth in their own docs and should not be
  hand-maintained in the README.
- Document config, auth, and safety gates in the README even when deeper docs
  exist.
- Keep credentials in config/env. Do not show examples that pass API keys or
  tokens in MCP tool arguments.
- For large or generated surfaces, explain how discovery works and where the
  authoritative generated catalogs live.
- Document distribution/version contracts when the repo ships npm launcher
  packages, GitHub Release binaries, Docker images, MCP registry metadata,
  plugin manifests, or release-please metadata.
- Mark maturity honestly: production, experimental, live-tested, read-only,
  destructive-capable, generated surface, or runtime product with scaffold/export
  lane.
- Always include verification commands that prove the binary, transport, and
  tool call path work.

## README Experience Goals

Treat the README as the first product surface. A good RMCP README should let a
new operator decide these things without opening the code:

1. Should I use this repo for my task?
2. Which install path fits my MCP client or deployment?
3. What is the first safe thing I can run?
4. Where do credentials live, and what must never be passed by a caller?
5. Which docs are curated and which docs are generated source of truth?

The first screen should be dense but humane. Avoid making users scroll past a
long architecture tour before they know the command that proves the server works.

## RMCP README Shape

Use the section below as product-first README adaptation guidance. For generated
projects, copy it into the root `README.md`, then replace bracketed placeholders
and delete optional sections that do not apply.

---

# [service-name]

[Optional badges: crates.io, npm, release, CI, container image.]

[One sentence: `[service-name]` is a Rust MCP server and CLI for ...]

[One short paragraph: explain the upstream service or local capability, who uses
it, and the single agent-facing outcome. Name the MCP tool(s), CLI binary, and
main transport in this paragraph.]

**30-second path:** [Install/run command] -> [safe CLI command] -> [safe MCP
tool call]. Keep this to one line or one compact sentence.

**Status:** [production | beta | experimental | runtime product with scaffold/export lane].
[Read-only | write-capable | destructive actions gated]. [Live-tested against
upstream | fixture-only | generated provider surface].

**Not for:** [Name the most tempting misuse cases. Examples: generic REST
gateway, scheduler, multi-tenant security boundary, replacement for upstream
service, arbitrary filesystem writer.]

## Contents

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

Use this table unless the project has a documented exception.

| Surface | Pattern | This repo |
|---|---|---|
| Repository | `<service>-rmcp` | `[service-name]-rmcp` |
| npm package | `<service>-rmcp` | `[service-name]-rmcp` |
| CLI / binary | `r<service>` | `r[service]` |
| MCP tool | `[service]` | `[tool-name]` |
| Env prefix | `[SERVICE]_MCP_*` plus service-specific vars | `[PREFIX]_*` |

If this repo is an exception, state why:

> [Example: The repo and CLI remain `cortex` because the product is broader
> than an MCP server; only the npm launcher uses `cortex-rmcp`.]

## Capabilities And Boundaries

[Two to five bullets that describe actual user-visible capabilities.]

- [Read/query capability.]
- [Write/control capability, if any.]
- [Generated or discovered operation surface, if any.]
- [Resources, prompts, MCP Apps, or other MCP primitives, if any.]
- [Operational status/doctor/setup capability.]

| This repo owns | Upstream owns | Explicitly out of scope |
|---|---|---|
| [MCP/CLI/REST projection, validation, auth policy, response shaping.] | [Source data, durable business state, upstream auth model.] | [Misuse cases from `Not for` plus anything callers often assume incorrectly.] |

## Install

Use this matrix before detailed install subsections so readers can pick a path
quickly.

| Path | Command | Best for | Notes |
|---|---|---|---|
| npm / npx | `npx -y [package-name] mcp` | Local MCP clients and quick trials. | Downloads or locates `[binary-name]`. |
| Release installer | `curl -fsSL .../scripts/install.sh \| bash` | Host installs without Node. | Installs to `~/.local/bin` by default. |
| Docker / Compose | `docker compose up -d` | Shared HTTP MCP/API deployments. | Requires explicit auth before non-loopback exposure. |
| Build from source | `cargo build --release` | Development and audits. | Uses the checked-out branch. |
| Plugin | `claude plugin install ...` | Client-specific packaged setup. | State whether it bundles MCP config, skills, hooks, or binaries. |

### npm / npx

Run the stdio MCP server or CLI without a manual binary install:

```bash
npx -y [package-name] --help
npx -y [package-name] mcp
```

MCP clients can use the same launcher:

```json
{
  "mcpServers": {
    "[tool-name]": {
      "command": "npx",
      "args": ["-y", "[package-name]", "mcp"]
    }
  }
}
```

The npm package downloads the matching `[binary-name]` binary from GitHub
Releases during `postinstall`. Keep the release tag aligned with
`packages/[package-name]/package.json`.

### Release Installer

```bash
curl -fsSL https://raw.githubusercontent.com/[owner]/[repo]/main/scripts/install.sh | bash
```

If the project ships Windows releases:

```powershell
irm https://raw.githubusercontent.com/[owner]/[repo]/main/scripts/install.ps1 | iex
```

### Build From Source

```bash
git clone https://github.com/[owner]/[repo]
cd [repo]
cargo build --release
./target/release/[binary-name] --help
```

Minimum supported Rust version: [version].
Supported release platforms: [linux x86_64, windows x86_64, macOS, etc.].

## Quickstart

### 1. Configure The Upstream Or Local Runtime

```bash
export [PREFIX]_URL="https://example.com"
export [PREFIX]_API_KEY="..."
export [PREFIX]_MCP_TOKEN="$(openssl rand -hex 32)"
```

Prefer `.env.example` or `config.soma.toml` for longer setups:

```bash
cp .env.example .env
$EDITOR .env
```

### 2. Run The CLI

```bash
[binary-name] status --json
[binary-name] [safe-read-command] --json
```

### 3. Run The MCP Server

Streamable HTTP:

```bash
[binary-name] serve
curl -sf http://127.0.0.1:[port]/health | jq .
```

stdio:

```bash
[binary-name] mcp
```

### 4. Make A First MCP Call

```bash
curl -s -X POST http://127.0.0.1:[port]/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Authorization: Bearer $[PREFIX]_MCP_TOKEN" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "[tool-name]",
      "arguments": {"action": "status"}
    }
  }' | jq .
```

## Client Configuration

Show the client paths the repo actually supports. Delete clients that do not
apply.

### Claude Code

```json
{
  "mcpServers": {
    "[tool-name]": {
      "command": "npx",
      "args": ["-y", "[package-name]", "mcp"]
    }
  }
}
```

If the repo ships a plugin, prefer the plugin path:

```bash
claude plugin install [path-or-marketplace-ref]
```

### Codex / Labby Gateway

State whether this server is normally registered through the Labby gateway, run
directly as stdio, or exposed as Streamable HTTP.

```toml
[mcp_servers.[tool-name]]
command = "npx"
args = ["-y", "[package-name]", "mcp"]
```

### Generic MCP JSON

```json
{
  "command": "[binary-name]",
  "args": ["mcp"],
  "env": {
    "[PREFIX]_URL": "https://example.com"
  }
}
```

Do not put API keys, passwords, OAuth secrets, SSH keys, or upstream bearer
tokens in MCP tool arguments. Use env/config or client secret storage.

## Runtime Surfaces

| Surface | Status | Entry point | Purpose |
|---|---:|---|---|
| MCP stdio | Required | `[binary-name] mcp` | Local child-process MCP clients. |
| MCP HTTP | Required for shared/server profiles | `[binary-name] serve`, `POST /mcp` | Streamable HTTP MCP with bearer/OAuth auth. |
| CLI | Required | `[binary-name] <command>` | Scriptable parity and debugging. |
| REST API | Optional | `/v1/*` or `/api/*` | Only when the server owns API workflows or state. |
| Web UI | Optional | `/` | Only when the server owns human-facing workflows. |
| Plugins | Optional | `plugins/[name]` | Claude Code, Codex, Gemini, monitors, and skills. |

For upstream-client servers, do not add REST/Web just because the upstream
service has an API. For application/platform servers, keep REST/Web thin and
backed by the same service layer as MCP and CLI.

## Transports

| Mode | Command | Endpoint | Notes |
|---|---|---|---|
| stdio MCP | `[binary-name] mcp` | stdin/stdout | Logs must go to stderr so JSON-RPC is not corrupted. |
| Streamable HTTP MCP | `[binary-name] serve` | `http://<host>:<port>/mcp` | Use bearer or OAuth before exposing beyond loopback. |
| CLI | `[binary-name] <command>` | local process | Uses the same service methods as MCP. |

## MCP Tool Reference

[Choose one model and delete the other.]

### Single Action-Dispatched Tool

The MCP server exposes one tool named `[tool-name]`. Pass the required `action`
argument to choose the operation:

```json
{
  "action": "status"
}
```

| Action | Scope | CLI | Parameters | Description |
|---|---|---|---|---|
| `status` | `[service]:read` | `[binary-name] status` | none | Return redacted runtime and upstream status. |
| `help` | public | `[binary-name] help` | optional `topic` | Return action reference. |
| `[action]` | `[service]:read` | `[binary-name] [command]` | `[params]` | [Description.] |
| `[write_action]` | `[service]:write` | `[binary-name] [command]` | `[params]` | [Description.] |

If actions are generated, include a compact summary table and link to the
generated schema:

> Full generated action schema: [`docs/MCP_SCHEMA.md`](docs/MCP_SCHEMA.md).

### Multiple MCP Tools

The MCP server exposes these tools:

| Tool | Purpose | Action model |
|---|---|---|
| `[tool-a]` | [Domain.] | `action` plus optional `subaction`. |
| `[tool-b]` | [Domain.] | [Simple actions/generated actions/etc.] |

Document each tool with a short action table. Put exhaustive parameter schemas
in generated docs.

### Resources, Prompts, And MCP Apps

Delete this section if the server only exposes tools.

| Primitive | Name / URI | Purpose |
|---|---|---|
| Resource | `[scheme]://...` | [Browsable context or app resource.] |
| Prompt | `[prompt-name]` | [Reusable workflow.] |
| MCP App | `ui://...` | [Progressive UI enhancement.] |

Non-UI and non-resource clients must continue to receive useful text or
structured tool results.

## CLI Reference

The CLI is the scripting/debugging parity surface for MCP actions.

```bash
[binary-name] help
[binary-name] status --json
[binary-name] [read-command] --json
[binary-name] [write-command] --flag value --json
[binary-name] mcp
[binary-name] serve
```

Business actions should map to the same service methods used by MCP. CLI-only
operator commands such as `serve`, `mcp`, `doctor`, `watch`, `setup`, `compose`,
or `db` are not MCP business actions.

Document known parity exceptions:

| Capability | Surface | Reason |
|---|---|---|
| MCP elicitation action | MCP-only | Requires live client interaction. |
| Setup/doctor/watch | CLI-only | Operator infrastructure, not business action. |

## Configuration

Configuration loads in this order:

1. CLI flags
2. Environment variables
3. `config.toml` or appdata config
4. Built-in defaults

Secrets belong in environment variables or `.env`; non-secret tuning belongs in
`config.toml`.

### Environment Variables

| Variable | Required | Default | Description |
|---|---:|---|---|
| `[PREFIX]_URL` | yes | - | Upstream service base URL. |
| `[PREFIX]_API_KEY` | yes | - | Upstream API credential. Never pass this in MCP arguments. |
| `[PREFIX]_MCP_HOST` | no | `127.0.0.1` | HTTP MCP bind host. |
| `[PREFIX]_MCP_PORT` | no | `[port]` | HTTP MCP bind port. |
| `[PREFIX]_MCP_TOKEN` | bearer | empty | Static bearer token. |
| `[PREFIX]_MCP_NO_AUTH` | no | `false` | Disable auth for loopback development only. |
| `[PREFIX]_NOAUTH` | no | `false` | Trusted-gateway no-auth mode for non-loopback deployments. |
| `[PREFIX]_MCP_AUTH_MODE` | no | `bearer` | `bearer` or `oauth`. |
| `[PREFIX]_MCP_PUBLIC_URL` | OAuth | empty | Public URL for OAuth metadata and callbacks. |
| `[PREFIX]_MCP_GOOGLE_CLIENT_ID` | OAuth | empty | Google OAuth client ID. |
| `[PREFIX]_MCP_GOOGLE_CLIENT_SECRET` | OAuth | empty | Google OAuth client secret. |
| `[PREFIX]_MCP_AUTHELIA_ISSUER_URL` | Authelia | empty | HTTPS Authelia OIDC issuer URL. |
| `[PREFIX]_MCP_AUTHELIA_CLIENT_ID` | Authelia | empty | Authelia OIDC client ID. |
| `[PREFIX]_MCP_AUTHELIA_CLIENT_SECRET` | Authelia | empty | Authelia OIDC client secret. |
| `[PREFIX]_MCP_GITHUB_CLIENT_ID` | GitHub | empty | GitHub OAuth App client ID. |
| `[PREFIX]_MCP_GITHUB_CLIENT_SECRET` | GitHub | empty | GitHub OAuth App client secret. |
| `[PREFIX]_MCP_AUTH_DEFAULT_PROVIDER` | no | first configured | Provider used when `provider` is omitted; automatic priority is Google, Authelia, GitHub. |
| `[PREFIX]_MCP_AUTH_ADMIN_EMAIL` | OAuth | empty | Initial/admin OAuth email. |
| `RUST_LOG` or `[PREFIX]_LOG` | no | `info` | Log filter. Stdio mode must keep protocol logs off stdout. |

When env variables are generated from code metadata, say so and point to the
generated reference:

> Generated env reference: [`docs/ENV.md`](docs/ENV.md).

### config.toml

```toml
[[service]]
name = "default"
url = "https://example.com"

[mcp]
host = "127.0.0.1"
port = [port]
server_name = "[tool-name]"
```

## Authentication

The HTTP MCP endpoint supports these policies:

| Policy | When | Effect |
|---|---|---|
| Loopback development | Loopback bind or `[PREFIX]_MCP_NO_AUTH=true` on loopback | No auth middleware; scope checks may be bypassed. |
| Bearer token | `[PREFIX]_MCP_TOKEN` set | `/mcp` requires `Authorization: Bearer <token>`. |
| OAuth | `[PREFIX]_MCP_AUTH_MODE=oauth` with at least one configured provider | Browser-based Google, Authelia, or GitHub login issues scoped JWT bearer tokens. |
| Trusted gateway | `[PREFIX]_NOAUTH=true` on non-loopback | Local auth/scope checks disabled because an upstream gateway enforces them. |

Document scopes:

| Scope | Grants |
|---|---|
| `[service]:read` | Read-only actions. |
| `[service]:write` | Mutating actions. |
| `[service]:admin` | Destructive/admin actions. |

The startup guard should refuse unauthenticated non-loopback binds unless
bearer, OAuth, or trusted-gateway mode is configured. `/health` should remain
unauthenticated.

## Safety And Trust Model

State concrete trust assumptions. Do not bury them in lower-level docs.

- Upstream credentials are read from env/config only; MCP callers never provide
  API keys, bearer tokens, passwords, SSH keys, or OAuth secrets as arguments.
- Destructive operations require explicit confirmation, for example
  `confirm=true`, `--confirm`, or a narrowly scoped environment override.
- URL, path, command, and query inputs are validated before reaching upstream
  tools or local processes.
- Local stdio MCP servers run with the user's permissions and are not a sandbox.
- Registry/plugin/remote MCP metadata is untrusted input until reviewed.
- Tool results, schemas, and descriptions from upstream MCP servers are
  untrusted output/input and should be redacted or bounded before display.

If the server exposes filesystem, shell, Docker, SSH, media download, syslog,
or arbitrary upstream passthrough behavior, add a service-specific subsection
describing the exact boundary and mitigations.

## Architecture

Use a small diagram for simple upstream clients:

```text
MCP client / CLI
       |
       v
[binary-name]
       |
       +-- MCP shim: parse JSON args -> service -> Value
       +-- CLI shim: parse argv -> service -> stdout
       |
       v
[Service]Service
       |
       v
[Service]Client
       |
       v
Upstream API / local runtime
```

For workspace/application servers, use a crate or module table:

| Path | Role |
|---|---|
| `crates/[service]-service` or `src/app.rs` | Business logic, validation, defaults, response shaping. |
| `crates/[service]-mcp` or `src/mcp/` | MCP schemas, tool dispatch, prompts/resources, scope checks. |
| `crates/[service]-cli` or `src/cli.rs` | CLI parser and output formatting. |
| `crates/[service]-api` or `src/api/` | Thin REST handlers, if shipped. |
| `crates/[service]-runtime` or `src/runtime.rs` | App state, auth policy, server wiring. |
| `apps/web` or `crates/[service]-web` | Web UI, if shipped. |

The thin-shim rule is strict:

1. Parse input at the surface.
2. Call the service.
3. Return or print the result.

No business logic, destructive gates, credential handling, path safety, response
normalization, or upstream defaults belong in MCP/CLI/REST shims.

## Distribution Contract

Delete rows that do not apply. Keep this section when more than one artifact or
manifest carries a version, binary name, package name, install command, or MCP
server identity.

| Artifact | File(s) | Must align with |
|---|---|---|
| Rust crate/binary | `Cargo.toml`, `Cargo.lock`, release assets | Git tag, binary names, install scripts, Docker entrypoint. |
| npm launcher | `packages/[package-name]/package.json`, `bin/*`, `lib/platform.js`, `scripts/install.js` | GitHub Release tag, release asset names, README install examples. |
| GitHub Releases | `.github/workflows/release.yml`, `scripts/install.sh`, `scripts/install.ps1` | Package versions, checksums, platform matrix, public installer commands. |
| Docker / OCI | `Dockerfile`, compose files, image labels | Binary names, exposed ports, healthcheck endpoint, registry metadata. |
| MCP registry | `server.json`, package metadata marker | Server name, package identifier, version, transport, env vars. |
| Plugins | `plugins/*`, `.claude-plugin/*`, `.agents/*`, skills | Runtime command, setup hooks, bundled binary policy, marketplace naming. |
| Generated docs | `docs/MCP_SCHEMA.md`, `docs/ENV.md`, `docs/generated/*` | Current action/env/schema/provider metadata. |

State the release invariant explicitly. Examples:

- npm package version, `server.json.version`, and GitHub Release tag must match.
- Release assets must use the names expected by both npm `platform.js` and
  `scripts/install.sh`.
- Plugin manifests do not carry hand-written semver when marketplace identity is
  derived from git/package metadata.
- README install commands must use canonical repository names, not GitHub rename
  redirects.

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Preferred local recipes:

```bash
just dev
just test
just lint
just fmt
just verify
```

If generated docs or schemas exist:

```bash
cargo xtask generate-docs
cargo xtask check-docs
cargo xtask check-schema-docs --check
```

## Verification

After changing the server, prove at least one path through each shipped surface.

```bash
# Binary and CLI
[binary-name] --version
[binary-name] status --json

# HTTP health
curl -sf http://127.0.0.1:[port]/health | jq .

# MCP tool call
curl -s -X POST http://127.0.0.1:[port]/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Authorization: Bearer $[PREFIX]_MCP_TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"[tool-name]","arguments":{"action":"status"}}}' | jq .

# Full local gates
just verify
```

For live upstream tests, list required env vars and mark tests ignored/gated by
default:

```bash
[PREFIX]_URL=https://example.com \
[PREFIX]_API_KEY=... \
cargo test --test live_smoke -- --ignored
```

## Deployment

Delete subsections that do not apply.

### Docker / Compose

```bash
docker compose up -d
curl -sf http://127.0.0.1:[port]/health | jq .
```

Document image names, volumes, host ports, healthcheck endpoints, user/group
IDs, and required env vars.

### Reverse Proxy

Expose only the intended MCP/API route, preserve Streamable HTTP headers, and
require TLS plus auth before internet exposure.

### Plugins

```bash
claude plugin install <path-or-marketplace-ref>
```

State whether the plugin bundles an MCP config, a binary, setup hooks, monitors,
skills, or only skills. Plugin manifests stay versionless when marketplace
versioning is derived from git/package metadata.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `401` from `/mcp` | Missing or wrong bearer token | Check `[PREFIX]_MCP_TOKEN` and client headers. |
| MCP stdio exits immediately | Missing required env/config | Run `[binary-name] doctor` or `status --json`. |
| JSON-RPC parse errors in stdio | Logs printed to stdout | Send logs to stderr or lower stdio log level. |
| Upstream calls fail | Bad URL, token, TLS, or network path | Run CLI status/health and inspect redacted config. |
| Generated docs drift | Action/env/schema changed | Run the repo's docs/schema generation checks. |

## Related Servers

Keep this section short: one line per sibling server or adjacent project, and
omit the current repo. Use linked product or repository names rather than
implementation trivia.

- [soma](https://github.com/jmagar/soma) - RMCP runtime for provider-backed MCP servers.
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

Start here:

- [`docs/QUICKSTART.md`](docs/QUICKSTART.md) - focused setup flow.
- [`docs/API.md`](docs/API.md) - REST/API surface, if shipped.
- [`docs/MCP_SCHEMA.md`](docs/MCP_SCHEMA.md) - generated MCP wire contract.
- [`docs/CONFIG.md`](docs/CONFIG.md) - config file and env loading.
- [`docs/ENV.md`](docs/ENV.md) - generated env reference, if present.
- [`docs/AUTH.md`](docs/AUTH.md) - bearer/OAuth/trusted-gateway model.
- [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) - production deployment.
- [`docs/TESTING.md`](docs/TESTING.md) - local, fixture, and live test strategy.
- [`plugins/README.md`](plugins/README.md) - plugin package layout, if shipped.

If generated catalogs are authoritative, say that clearly:

> Do not hand-maintain action, env, route, or schema inventories in this README.
> The generated docs above are the source of truth for the current branch.

## README Alignment Checklist

Use this before claiming a README is aligned with this guide:

- First screen names the upstream/local capability, MCP tool(s), binary, and
  fastest safe command.
- Product boundary is explicit, including "Not for" or equivalent language.
- Install matrix covers every supported distribution path and deletes unsupported
  paths.
- Quickstart includes one safe CLI call and one safe MCP call.
- Client config examples match the actual supported launch paths.
- Runtime surfaces table does not advertise REST, web, plugins, prompts,
  resources, or MCP Apps that are not shipped.
- MCP tool reference is complete enough to scan, with generated schema docs
  linked for exhaustive details.
- Config and auth sections explain where secrets live and forbid credentials in
  tool arguments.
- Safety/trust model covers destructive actions and service-specific risky
  inputs.
- Architecture section preserves the thin-shim rule across MCP, CLI, REST, and
  web surfaces.
- Distribution contract lists all version-bearing artifacts and their alignment
  invariants.
- Verification section proves every shipped surface, not just `cargo test`.
- Documentation section clearly separates curated README content from generated
  source-of-truth docs.

## License

MIT
