---
title: "synapse-application"
created: 2026-08-01
updated: 2026-08-01
status: implemented
---

# synapse-application

**Path:** `crates/synapse/application`
**Layer:** product application
**Package status:** private during extraction

## Purpose

`synapse-application` owns Synapse compatibility policy without owning infrastructure execution. It translates historical Flux and Scout requests into neutral `soma-ops` contracts and projects canonical outcomes back to legacy surfaces.

It does not depend on `crates/synapse/import`, Docker, SSH, MCP, HTTP, a database, or product configuration.

## Embedded contract set

At compile time the crate embeds and cross-validates:

- 59 canonical `OperationSpec` records;
- 59 product-owned `LegacyOperationBinding` records;
- 59 closed parameter schemas;
- 59 closed result schemas;
- 33 diagnostic surface projections.

Classification digests, schema identities, required fields, alternative groups, binding keys, and diagnostic coverage are checked together when the catalog is constructed.

## Public boundary

- `SynapseCatalog`
- `LegacyOperationBinding`
- `LegacyTool`, `LegacyAccess`, `LegacyTransport`, `LegacyPresentation`
- `NormalizedOperationRequest`
- `OperationSchemaContract`
- `DiagnosticProjection`
- `LegacyProjectedResult`
- `CompatibilityError`

## Compatibility flow

1. Resolve `(tool, action, subaction)` to one binding.
2. Remove legacy routing and presentation fields.
3. Reject fields outside the canonical parameter schema.
4. Validate required fields, alternatives, enums, bounds, and closed objects.
5. Return the canonical operation, parameters, required Synapse scope, and presentation.
6. Validate canonical results before projecting JSON or deterministic Markdown.
7. Project diagnostics only when globally mapped and declared by the operation.

## Generated surfaces

`SynapseCatalog::legacy_tool_schema` derives closed Flux and Scout input schemas from canonical parameter schemas and product-owned bindings. Help, MCP, REST, and CLI adapters can therefore consume one authoritative catalog instead of maintaining independent field lists.

## Verification

- embedded counts: 59 operations, 59 bindings, 33 diagnostics;
- shared help resolves through both tools;
- generated Flux and Scout schemas compile and reject unknown fields;
- real Docker build and Scout delta requests normalize successfully;
- missing fields, unknown fields, conflicting presentation, and invalid alternatives fail;
- mutation, command, text/artifact, status, and JSON results project deterministically;
- invalid result payloads fail before projection;
- operation-scoped diagnostics reject unrelated mapped codes;
- strict Clippy and warning-free rustdoc;
- sibling-test and architecture gates.
