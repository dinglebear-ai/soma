---
title: "Agent Runtime JSON Schemas"
created: 2026-08-05
updated: 2026-08-05
doc_type: "schema-index"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# JSON Schemas

All schemas use JSON Schema draft 2020-12. YAML manifests are validated by parsing YAML into a JSON-compatible value and validating the resulting tree.

- <code>common.schema.json</code>
- <code>agent-stack.schema.json</code>
- <code>context-manifest.schema.json</code>
- <code>snippet.schema.json</code>
- <code>labby-loadout.schema.json</code>
- <code>compiled-context.schema.json</code>
- <code>agent-run.schema.json</code>
- <code>lifecycle-event.schema.json</code>
- <code>synthesis-result.schema.json</code>

Schema validation is necessary but not sufficient. Semantic validation resolves paths, imports, packages, graph roots, snippets, capabilities, Incus resources, secret references, and output cross-references.
