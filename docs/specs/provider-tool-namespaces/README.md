---
title: "Provider Tool Namespaces"
created: 2026-08-02
updated: 2026-08-02
doc_type: "design-index"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "product"
source_of_truth: true
---

# Provider Tool Namespaces

This package defines the proposed canonical identity for every provider-backed
tool in Soma. A tool is no longer globally identified by one flat action name.
Its identity is the ordered pair `(provider, tool)`.

The motivating example is a Python provider stored as `nexus.py`:

```python
PROVIDER = provider(manifest_version=2, name="nexus", kind="python")

@tool(name="repos")
def repos(repo: str | None = None) -> dict:
    ...
```

The same identity is projected consistently:

```text
Canonical display  nexus.repos
CLI                soma nexus repos --repo soma
MCP                soma(provider="nexus", action="repos", repo="soma")
REST               POST /v1/providers/nexus/tools/repos
Palette            { provider: "nexus", tool: "repos" }
```

## Documents

| Artifact | Purpose |
|---|---|
| [SPEC.md](SPEC.md) | Design intent, scope, naming, routing, migration, and acceptance criteria. |
| [CONTRACT.md](CONTRACT.md) | Normative behavior and stable error/collision rules. |
| [TYPES.md](TYPES.md) | Proposed Rust domain, registry, invocation, and confirmation types. |
| [MODELS.md](MODELS.md) | CLI, MCP, REST, Palette, discovery, result, and error wire models. |
| [provider-tool-namespace.schema.json](provider-tool-namespace.schema.json) | Machine-readable identity, invocation, result, confirmation, and lookup-error schema. |
| [IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md) | Research-backed, dependency-ordered implementation and verification plan. |

## Locked Decisions

- `(provider, tool)` is canonical on every provider execution surface.
- Provider names are globally unique; tool names are unique within a provider.
- The declared provider name is authoritative. A filename is an inference and
  diagnostics source, not the durable API identity.
- Every provider kind uses the same namespace model.
- The built-in catalog migrates from its current provider name `static-rust`
  to `soma`; `static-rust` remains its provider kind.
- Custom REST routes remain optional. The canonical generic route always exists
  for REST-enabled tools.
- Runtime routing uses one generic Axum path, while live OpenAPI enumerates one
  concrete path operation per loaded provider tool.
- Provider-local aliases are allowed. New global aliases are not.
- Namespaced manifest v2 is a clean cutover. Soma does not load manifest v1 or
  provide flat provider CLI, MCP, or REST fallback routes.
- Existing concise built-in commands such as `soma status` remain deliberate
  product projections of `soma.status`; they are not provider compatibility.
- Palette, web, generated clients, paging, logs, and refresh events are part of
  the migration; joined display names are never parsed for dispatch.

## Research Corrections

The first draft was checked against current Soma internals and authoritative
protocol/framework documentation. That research found and corrected these
material gaps:

- MCP uses `action`, not `tool`, as its transport field.
- Flat compatibility indexes are unnecessary because this design deliberately
  makes a clean cutover to namespaced provider dispatch.
- The current built-in provider is named `static-rust`, so its migration to
  `soma` must be explicit.
- Non-executing inspection currently skips Python catalogs; it cannot claim to
  validate metadata that requires contained Python discovery.
- Palette, the web Tool Runner, the Rust client, shared MCP paging, and refresh
  diffs also use flat tool names.
- A generic OpenAPI Path Item cannot carry concrete per-provider metadata;
  live OpenAPI must enumerate concrete provider/tool paths.
- Native Windows CI and Python path classification require repair before the
  plan can claim cross-platform proof.

Primary references used by the revised plan include the
[MCP tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools),
[JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12),
[OpenAPI 3.1.1](https://spec.openapis.org/oas/v3.1.1.html), and
[Axum routing rules](https://docs.rs/axum/latest/axum/struct.Router.html#method.route).

## Status

These documents describe a proposed contract. They do not claim that `main`
already implements namespaced dispatch. Current implementation remains flat and
is documented in `docs/specs/dynamic-provider-runtime.md` and
`docs/PROVIDERS.md` until the implementation plan is completed.
