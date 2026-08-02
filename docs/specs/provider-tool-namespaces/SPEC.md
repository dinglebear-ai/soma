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

Soma currently indexes provider tools, CLI commands, MCP actions, and the
generic REST route by one global action string. Two providers cannot both
declare a useful generic name such as `status`, `search`, or `list`, even
though provider identity is already available in each catalog and
`ProviderCall` already carries separate provider and action fields.

Flat dispatch also obscures ownership. `soma repos` does not tell an operator
whether the implementation came from Nexus, a static Rust provider, or a
remote MCP provider.

## Goals

1. Make `(provider, tool)` the canonical execution identity.
2. Project that identity consistently through registry, CLI, MCP, REST,
   OpenAPI, discovery, errors, logs, traces, and generated docs.
3. Permit the same tool name in different providers.
4. Preserve existing concise built-in commands.
5. Give version 1 manifests a bounded, deterministic migration path.
6. Apply the model uniformly to static Rust, JSON, TypeScript, Python,
   LangChain, LlamaIndex, WASM, OpenAPI, and upstream MCP providers.

## Non-Goals

- Namespacing MCP prompts, resources, tasks, or elicitation in this change.
- Introducing provider-specific OAuth scopes.
- Supporting arbitrary-depth CLI command trees beyond provider and tool.
- Automatically generating global aliases for version 2 providers.
- Changing provider execution containment or capability policy.

## Canonical Identity

The canonical type is `ProviderToolId`:

```text
provider = ProviderId
tool     = ToolId
display  = provider + "." + tool
```

The display string is presentation only. Implementations must not concatenate
and later parse it to recover identity. Registry keys, calls, errors, and
surface resolvers carry the two typed fields.

Provider and tool identifiers use the existing lowercase ASCII identifier
shape: start with `a-z`; continue with lowercase ASCII, digits, `-`, or `_`;
do not end in a separator or contain adjacent/mixed separators. Existing valid
provider and action names therefore remain representable.

## Provider Namespace Source

`provider.name` is authoritative.

- Manifest-backed providers must declare it.
- Decorated Python providers use `provider(name=...)`.
- Legacy Python files without explicit identity infer a provider name from the
  normalized file stem.
- A source filename that differs from an explicit provider name produces a
  bounded `provider_filename_mismatch` warning in manifest v2's first release.
  A later contract revision may make it an error.
- Moving a file does not change its declared API identity.

The drop-in provider name `soma` is reserved for built-in tools. CLI-reserved
top-level words such as `serve`, `mcp`, `providers`, and `gateway` cannot be
used as drop-in provider namespaces.

## Manifest Versioning

Provider manifest schema version 2 adopts namespaced semantics:

- `provider.name` is the namespace.
- `tools[].name` is local to that namespace.
- `tools[].cli.command` and aliases are provider-local tool command segments.
- `tools[].rest.path`, when present, is an optional custom global route.
- Generic REST exposure is derived from provider and tool identity.
- MCP action metadata contains both provider and action.

Version 1 manifests continue loading for one release. The host converts them
to canonical identities internally and may expose their old flat CLI/MCP/REST
entry points only when a flat name maps to one identity.

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
```

Requirements:

- `soma <provider> --help` lists only that provider's tools.
- `soma <provider> <tool> --help` renders the tool schema and examples.
- Tool commands and aliases are unique only inside the provider.
- Built-in commands keep their existing short grammar (`soma status`,
  `soma providers inspect`) while resolving internally to provider `soma`.
- A v1 flat command may remain as a deprecated alias for one release if and
  only if it is unambiguous.

## MCP Projection

Soma continues exposing one action-dispatched MCP tool named `soma`. Its input
requires both fields for provider-backed tools:

```json
{
  "provider": "nexus",
  "action": "repos",
  "repo": "soma"
}
```

The term `action` remains the MCP transport spelling for the canonical tool
segment during this migration. MCP metadata, conditional input schemas, output
schemas, help, errors, and response discriminators must pair it with provider.

Built-ins use `"provider": "soma"`. During the compatibility release, omitting
provider is accepted only when the action resolves uniquely; the server emits
a structured deprecation warning. Ambiguous flat actions fail without choosing
a provider.

## REST Projection

Every REST-enabled provider tool has one canonical generic route:

```text
POST /v1/providers/{provider}/tools/{tool}
```

The request body is the tool argument object. Custom `rest.method` and
`rest.path` overlays remain available and globally collision-checked. They are
additional projections of the same `ProviderToolId`, not separate actions.

The existing `/v1/tools/{action}` route is a one-release compatibility route
with the same unique-resolution rule. Existing first-party direct routes such
as `/v1/status` remain supported and resolve internally to `soma.status`.

## Results and Errors

Successful surface envelopes identify ownership:

```json
{
  "provider": "nexus",
  "tool": "repos",
  "output": {},
  "request_id": "req_123",
  "progress": []
}
```

MCP structured output retains adapter discriminators and adds provider:

```json
{
  "_soma_provider": "nexus",
  "_soma_action": "repos",
  "output": {},
  "request_id": "req_123",
  "progress": []
}
```

Errors contain `provider` when known and `tool` when known. MCP adapters may
also include the transport spelling `action`. Lookup errors
distinguish an unknown provider, unknown tool within a known provider, and an
ambiguous legacy flat action.

## Collision Rules

- Duplicate provider names: reject.
- Duplicate tool names within one provider: reject.
- Same tool name in different providers: accept.
- Duplicate provider-local CLI command or alias: reject.
- Same CLI tool command in different providers: accept.
- Duplicate custom REST method/path: reject globally.
- Duplicate canonical REST route: impossible once provider and tool IDs are
  unique, but still asserted by registry construction.
- A drop-in provider using a reserved built-in namespace: reject.
- A legacy flat alias mapping to multiple canonical tools: mark ambiguous and
  do not register the alias.

## Discovery and Generated Artifacts

Provider inspection, capabilities, OpenAPI, help, plugin/skill generation, and
registry fingerprints include canonical identity. OpenAPI operations use a
stable operation ID such as `nexus_repos` and extensions:

```json
{
  "x-soma-provider": "nexus",
  "x-soma-tool": "repos"
}
```

Registry fingerprints include provider/tool pairs and every surface mapping so
namespace or alias changes invalidate generated snapshots.

## Acceptance Criteria

- Two providers can both expose `status` and dispatch correctly on every
  canonical surface.
- CLI, MCP, and REST select the same `ProviderToolId` for equivalent calls.
- Auth, confirmation, validation, paging, progress, and output validation use
  the selected registered tool without a second flat lookup.
- Unknown provider and unknown tool errors are distinct and structured.
- Legacy flat dispatch succeeds only for unique v1 actions and warns.
- Ambiguous legacy dispatch deterministically fails.
- Hot reload can add or remove a provider without corrupting namespace indexes.
- Non-executing inspection performs the same namespace/collision validation as
  the live registry without importing Python.
