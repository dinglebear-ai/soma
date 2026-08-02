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
PROVIDER = provider(name="nexus", kind="python")

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
```

## Documents

| Artifact | Purpose |
|---|---|
| [SPEC.md](SPEC.md) | Design intent, scope, naming, routing, migration, and acceptance criteria. |
| [CONTRACT.md](CONTRACT.md) | Normative behavior and stable error/collision rules. |
| [TYPES.md](TYPES.md) | Proposed Rust domain and registry types. |
| [MODELS.md](MODELS.md) | CLI, MCP, REST, discovery, result, and error wire models. |
| [provider-tool-namespace.schema.json](provider-tool-namespace.schema.json) | Machine-readable identity, invocation, result, and error schema. |
| [IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md) | Dependency-ordered implementation and verification plan. |

## Locked Decisions

- `(provider, tool)` is canonical on every execution surface.
- Provider names are globally unique; tool names are unique within a provider.
- The declared provider name is authoritative. A filename is an inference and
  diagnostics source, not the durable API identity.
- Every provider kind uses the same namespace model.
- Built-in Soma actions use the internal provider name `soma`, while existing
  concise built-in CLI and REST routes remain supported.
- Custom REST routes remain optional. The canonical generic route always exists
  for REST-enabled tools.
- Provider-local aliases are allowed. New global aliases are not.
- Version 1 flat dispatch receives one release of compatibility only when the
  flat name resolves to exactly one canonical identity.

## Status

These documents describe a proposed contract. They do not claim that `main`
already implements namespaced dispatch. Current implementation remains flat and
is documented in `docs/specs/dynamic-provider-runtime.md` and
`docs/PROVIDERS.md` until the implementation plan is completed.
