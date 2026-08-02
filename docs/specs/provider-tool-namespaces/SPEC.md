---
title: "Provider Tool Namespace Specification"
created: 2026-08-02
updated: 2026-08-02
doc_type: "specification"
status: "proposed"
owner: "soma"
scope: "product"
---

# Provider Tool Namespace Specification

## Problem

Soma currently indexes provider tools, CLI commands, MCP actions, the generic
REST route, Palette entries, web Tool Runner entries, refresh diffs, and parts
of response paging by one global action string. Two providers cannot both
declare a useful local name such as `status`, `search`, or `list`, even though
provider identity is already present in each catalog and invocation type.

Flat dispatch also obscures ownership. `soma repos` does not tell an operator
whether the implementation came from Nexus, a static Rust provider, or a
remote MCP provider. Worse, product control-plane and MCP-only actions are
currently intercepted by action segment alone; once duplicate local names are
allowed, that can execute the wrong behavior.

## Goals

1. Make `(provider, tool)` the canonical execution identity.
2. Project that identity consistently through provider-core, application
   policy, CLI, MCP, REST, OpenAPI, Rust/TypeScript clients, Palette, web,
   discovery, paging, errors, logs, traces, refresh events, and generated docs.
3. Permit the same tool name in different providers.
4. Preserve concise first-party commands and REST routes as explicit product
   projections.
5. Make manifest v2 and namespaced dispatch a clean cutover without flat
   provider compatibility paths.
6. Apply the model uniformly to static Rust, JSON, TypeScript, Python,
   LangChain, LlamaIndex, WASM, OpenAPI, and upstream MCP providers.
7. Preserve immutable generation, containment, authorization, confirmation,
   input/output validation, and response-size guarantees.

## Non-Goals

- Namespacing MCP prompts, resources, tasks, or elicitation primitives in this
  change. Their existing global collision rules remain.
- Introducing provider-specific OAuth scopes.
- Supporting arbitrary-depth CLI command trees beyond provider and tool.
- Automatically generating global aliases for version 2 providers.
- Adding SSH, Docker, Incus, SMB, or terminal broker services to Python.
- Putting lab-specific Nexus collectors into Soma product or scaffold code.
- Changing provider execution containment or capability policy except where
  identity must be carried through existing checks.

## Canonical Identity

The canonical type is `ProviderToolId`:

```text
provider = ProviderId
tool     = ToolId
display  = provider + "." + tool
```

The display string is presentation only. Implementations must not concatenate
and later parse it to recover identity. Registry keys, calls, errors, cursors,
surface resolvers, and audit data carry the two typed fields.

Provider and tool identifiers use the existing lowercase ASCII identifier
shape: start with `a-z`; continue with lowercase ASCII, digits, `-`, or `_`;
do not end in a separator or contain adjacent/mixed separators. This migration
does not add a new identifier-length limit because that would reject manifests
which are valid today. Deserialization must enforce the same grammar as
constructors; transparent serde must not bypass validation. Catalog/schema
size budgets provide operational bounds without changing identifier validity.

## Provider Namespace Source

`provider.name` is authoritative.

- Manifest-backed providers must declare it.
- Decorated Python v2 providers use
  `provider(manifest_version=2, name="...")`.
- The Python SDK emits manifest v2. The authoring version is distinct from the
  runner protocol, decorator metadata schema, native ABI, and componentization
  schema versions.
- Python files without an explicit name infer one from the normalized
  file stem during contained runtime discovery.
- A source filename that differs from an explicit provider name produces a
  bounded `provider_filename_mismatch` warning. Moving a file does not change
  its declared API identity.

Non-executing inspection cannot safely discover arbitrary Python decorators
without executing code. It therefore reports Python catalogs as
`runtime-validation-required`; it may reserve a provisional normalized file
stem, but must not claim tool or declared-name parity. `soma providers
validate|inspect|test` performs contained live discovery and is authoritative.
AST evaluation is not a substitute for executing arbitrary Python expressions.

## Built-In and Reserved Namespaces

The built-in catalog currently has provider name `static-rust` and provider
kind `static-rust`. Namespaced dispatch deliberately migrates its provider name
to `soma` while retaining its kind. Generated reports, skills, fixtures,
fingerprints, and reserved-name diagnostics migrate with it.

Drop-in providers cannot claim `soma` or any top-level token owned by the CLI
parser. The reserved set
is generated from one shared policy source rather than duplicated examples.
It includes product commands such as `greet`, `echo`, `status`, `help`,
`serve`, `mcp`, `doctor`, `watch`, `setup`, `package`, `providers`, `tools`,
and `openapi` when those commands exist.

Version 2 tool-local commands may use words such as `help` or `status` because
`soma nexus help` does not collide with the top-level parser. Provider-level
and tool-level `--help` remain parser operations and take precedence where
necessary.

## Manifest Versioning

Provider manifest schema version 2 establishes namespaced semantics:

- `provider.name` is the namespace.
- `tools[].name` is local to that namespace.
- `tools[].cli.command` and aliases are provider-local tool segments.
- `tools[].rest.path`, when present, is an optional custom global route.
- Generic REST exposure is derived from provider and tool identity.
- MCP action metadata contains both provider and action.

The Rust implementation keeps one `ProviderManifest` data model whose
`schema_version` must be `2`. Manifest v1 is rejected with a clear unsupported
version error. Providers never acquire implicit global aliases.

## Registry and Snapshot Semantics

The primary index is `ProviderToolId -> RegisteredTool`. Canonical REST requests
extract the pair from the path and use this index directly; no derived canonical
route entry is required. Separate indexes exist for provider-local CLI names,
and custom REST overlays. No flat provider-name resolver is built.

Custom REST route validation compares exact method/path and normalized route
shape, detects static shadowing of captures, rejects equivalent templates with
different parameter names, and reserves all infrastructure/canonical paths
before Axum router construction. Registry fingerprints continue using a
canonicalized complete-catalog representation; no parallel hand-maintained
fingerprint feed is added.

Refresh added/removed/surface-change events use sorted provider/tool pairs,
not tool-name sets, so adding `alpha.status` beside `beta.status` remains
observable.

## Invocation and Confirmation

Canonical requests enter the application as a structured `ProviderToolId`.
Product control-plane behavior and MCP-only elicitation branch only on the full
identity, such as `soma.python_worker_cancel` or `soma.elicit_name`; a dynamic
provider tool with the same local name is ordinary provider code.

Interactive confirmation uses two stages:

1. Preflight resolves identity and destructive metadata from one immutable
   snapshot, then issues a confirmation challenge bound to the provider/tool
   pair and snapshot fingerprint. It does not hold a Python generation lease
   while waiting for human input.
2. Final dispatch re-resolves the canonical identity, compares policy-relevant
   metadata/fingerprint, and rejects stale confirmation with
   `stale_provider_confirmation` when the target changed. It then acquires a
   dispatch lease and uses one registered entry through auth, validation,
   capability checks, execution, output validation, paging, and envelope
   construction.

Registry locks are not held during provider execution. Abandoned prompts do not
pin retiring generations.

## CLI Projection

The canonical grammar is:

```text
soma <provider> <tool> [tool flags]
```

Examples:

```bash
soma nexus repos --repo soma
soma nexus services --device squirts --service swag
soma weather status
soma providers test nexus repos --json '{}'
```

Soma currently uses a hand-written parser, not Clap. The implementation extends
that parser with a structured `provider` and `tool` command variant; it does not
add Clap unless a measured dependency/size decision is separately approved.

Requirements:

- Built-in parser-owned commands match before provider namespaces.
- `soma <provider> --help` lists only that provider's tools from one immutable
  snapshot.
- `soma <provider> <tool> --help` renders that tool's schema and examples.
- Tool commands and aliases are unique only inside the provider.
- Built-in commands keep their existing short grammar while resolving
  internally to `soma.*`.
- Provider-local aliases are resolved only after selecting the provider.
- Destructive prompts name `provider.tool`, never only the local tool segment.

Help is internally consistent within the snapshot used to render it. A later
separate invocation may legitimately observe a newer generation.

## MCP Projection

Soma continues exposing one action-dispatched MCP tool named `soma`. Canonical
input uses `provider` plus the MCP transport spelling `action`:

```json
{
  "provider": "nexus",
  "action": "repos",
  "repo": "soma"
}
```

The input schema is JSON Schema Draft 2020-12. It contains one complete object
branch per `(provider, action)` pair. Each branch requires both discriminator
fields with `const`, incorporates that tool's complete input schema and paging
metadata, and closes composition with `unevaluatedProperties: false` where
appropriate. Parameter definitions are never merged globally with first-wins
behavior.

Successful `structuredContent` is an object envelope containing
`_soma_provider`, `_soma_action`, `output`, `request_id`, and `progress`.
Provider results may remain primitive under `output`. Output schema branches
discriminate on both fields and include every structured success/error/page
shape that the server can emit. A JSON text content block mirrors normalized
`structuredContent` for older clients.

Unknown provider/action pairs are tool-result input/execution failures with
`isError: true`, not protocol-level "unknown MCP tool" errors. Protocol errors
remain reserved for the actual unknown tool name `soma`, malformed JSON-RPC,
authorization/scope denial, and server/protocol defects according to the
existing policy.

After a successful generation swap that changes the advertised schema, Soma
emits `notifications/tools/list_changed` to subscribed clients. Rejected
reloads retain the prior schema/fingerprint and emit no notification. Tests
cover the protocol versions actually negotiated by pinned `rmcp` rather than
claiming unsupported versions.

## REST and OpenAPI Projection

Every REST-enabled provider tool has one canonical route:

```text
POST /v1/providers/{provider}/tools/{tool}
```

The request body is the tool argument object. Runtime uses one generic Axum
capture route, validates decoded path segments into typed IDs, and dispatches
directly through the canonical tool index. Invalid percent encoding, encoded
separators/dot segments, Unicode, uppercase, and invalid identifiers fail
before lookup.

Live OpenAPI does not pretend one generic Operation Object belongs to many
concrete tools. For each loaded REST tool it enumerates the concrete path, for
example `/v1/providers/nexus/tools/repos`, with that tool's request/output
schemas, concrete `x-soma-provider` and `x-soma-tool`, and a globally unique,
collision-safe operation ID derived from structured identity. Simple underscore
concatenation is insufficient because `a_b.c` and `a.b_c` collide. The generic
path template remains discovery metadata rather than a fixed-identity
Operation Object.

Custom `rest.method` and `rest.path` overlays remain globally collision-checked
additional projections of the same identity. GET, HEAD, and DELETE overlays do
not rely on request bodies; inputs use declared path/query mappings. Body-based
tools use POST, PUT, or PATCH.

Canonical routes and v2 custom routes return the identity-bearing v2 envelope.
Soma does not expose `/v1/tools/{action}` for provider tools. Existing
first-party direct routes may remain deliberate projections of built-in
`soma.*` identities and preserve their documented response shapes.

REST status mapping is centralized: malformed identity/input is `400`, unknown
provider/tool is `404`, and authorization is `401/403` under existing policy.

## Palette, Web, Clients, and Paging

Palette DTOs carry provider and tool separately for catalog entries, schema
lookup, confirmation, and execution. Any joined launcher/display ID is opaque
presentation data and is never parsed. Two providers exposing `status` both
remain visible and executable.

The Rust client, generated TypeScript client, web Tool Runner, embedded web
asset mirror, generated action metadata, and package documentation use
canonical routes and composite IDs. UI deduplication keys on provider/tool,
not local tool name.

Response paging cache entries and cursors are bound to canonical identity.
Every page returns the pair; a continuation supplying a conflicting identity
is rejected and never re-executes the provider.

## Results, Errors, and Observability

Canonical successful surface envelopes identify ownership:

```json
{
  "provider": "nexus",
  "tool": "repos",
  "output": {},
  "request_id": "req_123",
  "progress": [],
  "warnings": []
}
```

MCP uses its adapter discriminators inside `structuredContent`:

```json
{
  "_soma_provider": "nexus",
  "_soma_action": "repos",
  "output": {},
  "request_id": "req_123",
  "progress": [],
  "warnings": []
}
```

Errors contain `provider` and `tool` when known; MCP may also include `action`.
Lookup errors distinguish unknown provider, unknown tool in a known provider,
and stale confirmation.

Logs, traces, metrics, authz/capability decisions, refresh events, and warnings
carry both fields but never raw inputs or secrets.

## Collision Rules

- Duplicate provider names: reject.
- Duplicate tool names within one provider: reject.
- Same tool name in different providers: accept.
- Duplicate provider-local CLI command or alias: reject.
- Same CLI tool command in different providers: accept.
- Duplicate/equivalent/overlapping custom REST route shape: reject before
  router construction.
- A custom route shadowed by infrastructure or canonical routes: reject.
- Canonical routes derive directly from unique typed identity.
- A drop-in provider using a reserved built-in/root namespace: reject.

## Discovery and Generated Artifacts

Provider inspection, capabilities, OpenAPI, CLI/Palette/web catalogs,
plugin/skill generation, refresh events, and registry fingerprints include
canonical identity. Discovery includes manifest semantics, input/output schema,
surface mappings, aliases, generation, and fingerprint.

It exposes both the conceptual canonical URL template and each concrete loaded
URL. Generated outputs sort identities deterministically without reordering the
live catalog presentation unless explicitly intended.

## Cutover Policy

The first published Soma release containing namespaced dispatch accepts
manifest v2 provider catalogs and canonical provider/tool calls only. The
implementation migrates all in-repository manifests, examples, generated
artifacts, SDK output, and built-in identities in the same change. Manifest v1
and flat provider CLI/MCP/REST calls fail clearly; Soma does not add temporary
resolvers, deprecation headers, telemetry, or a later removal phase for them.

Concise first-party commands and direct routes are retained only when explicitly
listed as product projections of built-in `soma.*` identities.

## Nexus Trial Boundary

Nexus has two layers:

1. Deterministic fixture-backed provider tests prove manifest v2 and normalized
   CLI/MCP/REST/Palette identity, output, error, paging, and hot-reload
   behavior in CI with no network or lab dependency.
2. An opt-in trusted-local live smoke may query repositories, shares, services,
   keys, nginx, Docker, and Incus through narrow collector interfaces. It is
   read-only, scope/admin/redaction aware, and never part of default CI.

The current Python broker does not expose SSH, terminal, Docker, Incus, or SMB
services. Fixture parity does not claim brokered production support for those
collectors.

## Acceptance Criteria

- Two providers can both expose `status` and dispatch correctly on CLI, MCP,
  REST, Palette, web/client projections, paging, and discovery.
- CLI, MCP, REST, and Palette select the same `ProviderToolId` for equivalent
  calls.
- Product control-plane and MCP-only actions require the full built-in identity.
- Auth, final confirmation, validation, paging, progress, and output validation
  use one final registered entry and generation lease.
- A refresh between confirmation preflight and execution cannot reuse stale
  confirmation against a changed target.
- Unknown provider/tool errors are stable.
- Successful schema swaps emit one tools-list-changed notification; rejected
  swaps retain the prior snapshot and emit none.
- Non-executing inspection states its Python visibility limits and agrees with
  live validation for every statically visible collision.
- Concrete OpenAPI operations have collision-safe IDs and fixed identity
  extensions; route-shape validation prevents Axum panics/shadowing.
- Native Linux and Windows execution, Python package paths, one-shot and
  persistent runners, and deterministic Nexus parity are exercised in CI.
