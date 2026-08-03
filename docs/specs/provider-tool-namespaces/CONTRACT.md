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
7. Drop-in providers MUST NOT claim `soma` or a parser-owned top-level
   namespace.

## Manifest Semantics

1. Provider manifests MUST declare `schema_version: 2`; manifest v1 MUST be
   rejected as unsupported.
2. CLI commands and aliases MUST be provider-local.
3. The Python authoring API MUST emit manifest v2. Provider manifest version
   MUST NOT be confused with runner protocol, decorator metadata, native ABI,
   or component schema versions.
4. The built-in provider name MUST migrate from `static-rust` to `soma` while
   its provider kind remains `static-rust`.

## Registry Construction

The registry MUST build these logical indexes atomically with each snapshot:

```text
tools               ProviderToolId -> RegisteredTool
cli                 (provider, local command) -> ProviderToolId
custom_rest         (method, path) -> ProviderToolId
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
- Aliases MUST remain provider-local. Global provider-tool aliases MUST NOT be
  added.
- Built-in CLI commands MAY remain top-level projections of `soma.*` identities.

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
- Canonical/custom provider routes MUST return the identity-bearing envelope.
- Flat `/v1/tools/{action}` provider routes MUST NOT be exposed.
- Existing first-party direct routes MAY remain explicit projections of
  built-in `soma.*` identities and preserve their documented response shape.
- REST status mapping MUST be centralized: invalid identity/input `400`,
  unknown provider/tool `404`, and auth according to existing `401/403` policy.

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

## Palette, Web, and Clients

- Palette catalog, schema lookup, confirmation, and execution DTOs MUST carry
  provider and tool separately.
- Web/client action keys and deduplication MUST use canonical identity.
- Rust and generated TypeScript clients MUST call canonical provider/tool
  routes.
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

## Cutover

1. Namespaced dispatch MUST ship as a clean cutover.
2. Manifest v1 and provider-less flat CLI, MCP, and REST calls MUST NOT be
   accepted by the namespaced implementation.
3. All in-repository manifests, SDK output, examples, generated artifacts, and
   built-in identities MUST migrate in the same change.
4. Unsupported manifest versions and non-canonical calls MUST fail clearly.
5. Explicit built-in concise commands and direct routes are product APIs, not
   provider compatibility fallbacks.

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
| `unsupported_provider_manifest_version` | Provider manifest is not schema v2. |
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
- Canonical dispatch, discovery, and generated outputs all resolve through the
  same immutable registry semantics.
