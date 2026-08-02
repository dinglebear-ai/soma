---
title: "Provider Tool Namespace Contract"
created: 2026-08-02
updated: 2026-08-02
doc_type: "contract"
status: "proposed"
owner: "soma"
scope: "product"
source_of_truth: true
---

# Provider Tool Namespace Contract

The key words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY are normative.

## Identity

1. A provider-backed tool MUST be identified by `provider` and `tool`.
2. Registries MUST store the identity as two typed fields.
3. A joined display name such as `nexus.repos` MUST NOT be parsed for dispatch.
4. Provider names MUST be globally unique in one registry snapshot.
5. Tool names MUST be unique within their provider and MAY repeat across
   providers.
6. Drop-in providers MUST NOT claim the built-in `soma` namespace or another
   reserved top-level CLI namespace.

## Registry Construction

The registry MUST build these logical indexes atomically with each snapshot:

```text
tools          ProviderToolId -> RegisteredTool
cli            (provider command, tool command) -> ProviderToolId
rest           (method, path) -> ProviderToolId
legacy_flat    flat action -> Unique(ProviderToolId) | Ambiguous
```

Registry construction MUST fail before publication for duplicate providers,
duplicate tools within a provider, duplicate provider-local CLI names, invalid
identifiers, reserved namespaces, or duplicate custom REST routes.

A refresh failure MUST retain the last valid immutable snapshot.

## Invocation

1. Surface adapters MUST resolve one `ProviderToolId` before authorization or
   execution.
2. Authorization, confirmation, input validation, provider lookup, execution,
   output validation, paging, and audit data MUST use that same registered
   entry and snapshot lease.
3. No adapter may resolve a flat name and later independently look up provider
   ownership.
4. `ProviderCall` MUST carry canonical identity, parameters, surface, snapshot,
   and invocation context.
5. Provider adapters MUST receive the provider-local tool name expected by the
   provider implementation.

## CLI

- The canonical provider grammar MUST be `soma PROVIDER TOOL`.
- Provider and tool help MUST be generated from the same registry snapshot used
  for dispatch.
- `cli.command` and `cli.aliases` in manifest v2 MUST be provider-local.
- New global aliases MUST NOT be registered.
- Built-in CLI commands MAY remain top-level aliases for `soma.*` identities.

## MCP

- The `soma` MCP tool schema MUST require `provider` and `action` for canonical
  calls.
- Conditional parameter schemas MUST discriminate on both fields.
- Action metadata and output-schema metadata MUST include provider.
- Successful non-paged output MUST contain `_soma_provider` and `_soma_action`.
- Provider errors MUST contain both identity components when known. MCP may
  additionally project the tool segment as `action`.

## REST

- A REST-enabled tool MUST be reachable at
  `POST /v1/providers/{provider}/tools/{tool}`.
- A custom route MAY use another method/path and MUST resolve to the same
  canonical identity.
- Custom method/path pairs MUST be globally unique.
- Generated OpenAPI MUST document canonical and custom routes without assigning
  the same operation ID to two identities.

## Compatibility

For one release after manifest v2 ships:

1. Version 1 manifests MUST load through an explicit compatibility adapter.
2. `/v1/tools/{action}`, provider-less MCP actions, and old flat CLI commands
   MAY resolve only when `legacy_flat[action]` is `Unique`.
3. A successful legacy call MUST emit a structured deprecation warning naming
   the canonical replacement.
4. `Ambiguous` MUST return `ambiguous_legacy_action`; it MUST NOT use load
   order, provider kind, or lexical order as a tie-breaker.
5. Version 2 providers MUST NOT acquire implicit global aliases.

The following breaking release removes the flat compatibility paths and the
version 1 manifest adapter.

## Stable Error Codes

| Code | Meaning |
|---|---|
| `invalid_provider_name` | Provider identifier violates the identifier grammar. |
| `invalid_tool_name` | Tool identifier violates the identifier grammar. |
| `reserved_provider_namespace` | Drop-in provider claims a reserved namespace. |
| `duplicate_provider_name` | Two catalogs declare the same provider. |
| `duplicate_provider_tool` | One provider declares the same tool twice. |
| `duplicate_provider_cli_command` | A provider-local command/alias collides. |
| `duplicate_rest_route` | Two tools claim the same custom method/path. |
| `unknown_provider` | No provider exists in the active snapshot. |
| `unknown_provider_tool` | Provider exists but does not contain the tool. |
| `ambiguous_legacy_action` | A flat compatibility name maps to multiple tools. |
| `legacy_action_removed` | Caller uses a compatibility path after removal. |
| `provider_filename_mismatch` | Explicit provider identity differs from source stem. |

## Invariants

- Registry fingerprints change when any canonical identity or surface mapping
  changes.
- Serialization is deterministic: catalogs sort by provider, tools by tool.
- Case-sensitive identity is preserved; implementations MUST NOT perform
  locale-sensitive normalization.
- Non-executing inspection and live registry validation MUST agree on every
  collision visible without executing provider code.
- Windows path separators or filename case behavior MUST NOT affect declared
  provider identity.
