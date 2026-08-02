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
2. Registries MUST store the identity as two validated typed fields.
3. A joined display name such as `nexus.repos` MUST NOT be parsed for dispatch.
4. Provider names MUST be globally unique in one registry snapshot.
5. Tool names MUST be unique within their provider and MAY repeat across
   providers.
6. Provider/tool deserialization MUST enforce the constructor grammar.
7. Drop-in providers MUST NOT claim `soma`, the compatibility-reserved
   `static-rust`, or a parser-owned top-level namespace.

## Manifest Semantics

1. Manifest schema v1 and v2 MUST be distinguished by required, mutually
   exclusive `schema_version` const branches.
2. One Rust manifest model MAY represent both versions; semantic behavior MUST
   be explicit as `V1Flat` or `V2Namespaced`.
3. V2 CLI commands/aliases MUST be provider-local.
4. V1 surface spellings MUST be recorded before normalization.
5. The Python authoring API MUST select manifest v2 explicitly during the
   compatibility window. Provider manifest version MUST NOT be confused with
   runner protocol, decorator metadata, native ABI, or component schema
   versions.
6. The built-in provider name MUST migrate from `static-rust` to `soma` while
   its provider kind remains `static-rust`.

## Registry Construction

The registry MUST build these logical indexes atomically with each snapshot:

```text
tools               ProviderToolId -> RegisteredTool
cli                 (provider, local command) -> ProviderToolId
custom_rest         (method, path) -> ProviderToolId
legacy_cli_command  flat command/alias -> Unique | Ambiguous
legacy_mcp_action   flat action         -> Unique | Ambiguous
legacy_rest_action  flat action         -> Unique | Ambiguous
```

The canonical REST route MUST extract `ProviderToolId` and use `tools`
directly. It MUST NOT require a duplicate canonical route index.

Registry construction MUST fail before publication for duplicate providers,
duplicate tools within a provider, duplicate provider-local CLI names, invalid
identifiers, reserved namespaces, duplicate/equivalent custom REST routes, or
custom routes shadowed by infrastructure/canonical routes.

Custom route comparison MUST cover exact method/path, normalized template
shape, capture-name equivalence, and static/capture shadowing before Axum router
construction.

A refresh failure MUST retain the last valid immutable snapshot. Refresh diffs
MUST identify tools by sorted provider/tool pairs. Fingerprints MUST be derived
from one deterministic complete-catalog representation; implementations MUST
NOT maintain a second manually synchronized fingerprint inventory.

## Invocation

1. Canonical surface adapters MUST submit a structured `ProviderToolId`; they
   MUST NOT silently fall back to a flat resolver.
2. Product control-plane and MCP-only behavior MUST branch on the complete
   built-in identity, never the tool segment alone.
3. Final authorization, confirmation validation, input validation, provider
   lookup, execution, output validation, paging, and envelope construction MUST
   use one final registered entry, snapshot, and dispatch lease.
4. Registry locks MUST NOT be held during provider execution.
5. Provider adapters MUST receive the provider-local tool name expected by the
   implementation plus canonical provider/snapshot context.
6. Application policy MUST distinguish the product-neutral provider invocation
   from the authorized/prepared application execution type.

## Confirmation

1. Interactive preflight MUST bind its challenge to provider, tool, snapshot
   fingerprint, and policy-relevant destructive metadata.
2. Preflight MUST NOT hold a provider generation lease while awaiting user or
   MCP elicitation input.
3. Final dispatch MUST re-resolve the canonical identity and compare the proof
   with current metadata.
4. A changed target or destructive policy MUST return
   `stale_provider_confirmation` and require a new confirmation.
5. Confirmation prompts and audit records MUST name `provider.tool`.

## CLI

- The canonical provider grammar MUST be `soma PROVIDER TOOL`.
- The existing hand-written parser SHOULD be extended structurally; adding Clap
  requires a separate measured dependency decision.
- Parser-owned built-ins MUST win before provider namespaces.
- Reserved provider names MUST come from one shared policy source.
- Provider/tool help MUST use one immutable snapshot, without claiming a later
  command observes the same generation.
- V2 aliases MUST remain provider-local. New global aliases MUST NOT be added.
- Built-in CLI commands MAY remain top-level projections of `soma.*` identities.
- Human legacy warnings MUST go to stderr. Machine output MUST remain parseable
  and expose structured warnings.

## MCP

- The `soma` MCP tool schema MUST use `provider` and `action` for canonical
  calls.
- Input schema MUST be explicit JSON Schema Draft 2020-12.
- Every canonical branch MUST require `provider` and `action`, constrain both
  with `const`, and incorporate that tool's complete input schema.
- Global first-wins merging of parameter schemas MUST NOT be used.
- Successful non-paged `structuredContent` MUST contain `_soma_provider` and
  `_soma_action`.
- Success, page, and structured-error output branches MUST match every shape
  the advertised output schema permits.
- A normalized JSON text content block SHOULD mirror `structuredContent` for
  backward compatibility.
- Unknown provider/action pairs inside `soma` MUST be tool-result errors with
  `isError: true`, not unknown-MCP-tool protocol errors.
- Paging cursors/cache entries MUST bind provider and action and reject identity
  substitution without re-execution.
- A successful schema-changing generation swap MUST emit
  `notifications/tools/list_changed` to subscribers. A rejected swap MUST emit
  none.

## REST

- A REST-enabled tool MUST be reachable at
  `POST /v1/providers/{provider}/tools/{tool}`.
- Decoded path segments MUST validate as typed IDs before lookup.
- A custom route MAY use another method/path and MUST resolve to the same
  canonical identity.
- GET, HEAD, and DELETE custom routes MUST NOT depend on request-body semantics.
- V2 canonical/custom routes MUST return the identity-bearing v2 envelope.
- V1 custom, flat compatibility, and existing first-party direct routes MUST
  preserve their documented response shape during compatibility.
- REST status mapping MUST be centralized: invalid identity/input `400`,
  unknown provider/tool `404`, ambiguous legacy name `409`, and auth according
  to existing `401/403` policy.

## OpenAPI

- Runtime MAY use one generic capture route, but live OpenAPI MUST enumerate a
  concrete path operation for each loaded REST provider tool when publishing
  fixed per-tool schemas and metadata.
- Each concrete operation MUST have globally unique, collision-safe
  `operationId`, `x-soma-provider`, and `x-soma-tool` values.
- Operation IDs MUST derive from structured identity with an injective encoding
  or stable collision suffix; naive underscore concatenation is forbidden.
- Any templated path documented as an Operation MUST declare every path
  parameter with `in: path` and `required: true`.
- Compatibility operations MUST set `deprecated: true`.

## Palette, Web, and Clients

- Palette catalog, schema lookup, confirmation, and execution DTOs MUST carry
  provider and tool separately.
- Web/client action keys and deduplication MUST use canonical identity.
- Rust and generated TypeScript clients MUST call canonical routes for v2
  tools and retain explicit compatibility methods only for v1 callers.
- Embedded/mirrored web assets and their source MUST update atomically through
  the existing generation path.

## Inspection and Python

- Non-executing inspection MUST NOT import or execute Python.
- It MUST report Python catalog identity/tools as runtime-validation-required
  when they are not statically knowable.
- A provisional filename-derived namespace MAY be reserved for diagnostics but
  MUST NOT be represented as confirmed declared identity.
- Live contained discovery MUST enforce declared provider/tool identity and all
  collisions before publication.
- Static manifest/provider kinds MUST produce matching non-executing and live
  collision outcomes for every statically visible rule.

## Compatibility

During the compatibility release:

1. Version 1 manifests MUST load through explicit semantic normalization.
2. Each legacy CLI/MCP/REST resolver MAY dispatch only its own `Unique` entry.
3. `Ambiguous` MUST fail without load-order, provider-kind, or lexical
   tie-breaking.
4. Successful legacy calls MUST name the canonical replacement in a structured
   warning.
5. REST MUST emit `Deprecation` and a `rel="deprecation"` documentation link;
   it MUST emit `Sunset` once a removal date is known.
6. V2 providers MUST NOT acquire implicit global aliases.
7. Legacy-use metrics MUST be bounded by surface and canonical identity and
   MUST NOT include users, requests, parameters, or secrets.

Removal MUST be a separately tracked breaking release after at least one
published Soma compatibility release, a compatible published Python SDK,
host/SDK version-matrix proof, migration documentation, and an adoption-metric
gate. It MUST NOT be required to close the implementation epic.

## Stable Error Codes

| Code | Meaning |
|---|---|
| `invalid_provider_name` | Provider identifier violates the grammar. |
| `invalid_tool_name` | Tool identifier violates the grammar. |
| `reserved_provider_namespace` | Drop-in provider claims a reserved namespace. |
| `duplicate_provider_name` | Two catalogs declare the same provider. |
| `duplicate_provider_tool` | One provider declares the same tool twice. |
| `duplicate_provider_cli_command` | A provider-local command/alias collides. |
| `duplicate_rest_route` | Two tools claim the same custom method/path. |
| `ambiguous_rest_route_shape` | Custom route templates overlap/equate. |
| `shadowed_rest_route` | Custom route is unreachable behind a reserved route. |
| `unknown_provider` | No provider exists in the active snapshot. |
| `unknown_provider_tool` | Provider exists but does not contain the tool. |
| `ambiguous_legacy_cli_command` | Flat CLI spelling maps to multiple CLI tools. |
| `ambiguous_legacy_mcp_action` | Provider-less MCP action maps to multiple tools. |
| `ambiguous_legacy_rest_action` | Flat REST action maps to multiple tools. |
| `legacy_action_removed` | Caller uses compatibility after removal. |
| `provider_filename_mismatch` | Explicit provider identity differs from source stem. |
| `python_runtime_validation_required` | Non-executing inspection cannot confirm Python catalog. |
| `stale_provider_confirmation` | Target/policy changed after preflight confirmation. |

## Invariants

- Registry fingerprints change when canonical identity or any serialized
  surface mapping changes.
- Deterministic fingerprint ordering MUST NOT silently change presentation
  order unless that is an explicit contract.
- Case-sensitive identity is preserved; locale-sensitive normalization is
  forbidden.
- Windows path separators or filename case behavior MUST NOT affect declared
  provider identity.
- Logs/traces/events include provider and tool when known and exclude raw
  arguments and secrets.
- Canonical dispatch, compatibility dispatch, discovery, and generated outputs
  all resolve through the same immutable registry semantics.
