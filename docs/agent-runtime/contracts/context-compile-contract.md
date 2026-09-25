---
title: "Context Compile Contract"
created: 2026-08-05
updated: 2026-08-05
doc_type: "contract"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Context Compile Contract

## Operations

### <code>context.validate</code>

Input: manifest path or serialized manifest.

Output: schema result, semantic errors, warnings, resolved imports, source availability summary, and manifest digest. It MUST NOT create a compiled context.

### <code>context.compile</code>

Input:

- manifest or manifest reference;
- optional view;
- task;
- roots and parameters;
- repository revision and dirty-state policy;
- time window;
- caller identity;
- budgets;
- snapshot mode.

Output:

- context ID and generation;
- compilation status;
- generated manifest;
- source and freshness report;
- selected query plan;
- counts and budgets;
- warnings and exclusions;
- materialization handles requested synchronously.

### <code>context.get</code>

Returns the immutable generated manifest and summary. Raw selected evidence is accessed through authorized query or materialization operations.

### <code>context.materialize</code>

Input: context ID, format, selection, destination policy, and budgets.

Output: materialization receipt with path or resource handle, size, digest, content type, and item count.

### <code>context.compare</code>

Input: two context IDs and dimensions.

Output: bounded differences with canonical references and truncation.

### <code>context.enrich</code>

Input: parent context ID and new evidence or completed research jobs.

Output: new child context generation. Parent mutation is forbidden.

## Invariants

- Compilation is read-only against canonical evidence stores.
- The generated manifest is immutable after publication.
- Authorization filters every retrieval lane before fusion.
- Vector or graph projections are not sufficient evidence without canonical hydration when canonical records are available.
- Every selected item has a canonical reference and classification.
- Excluded sensitive data cannot be reintroduced by graph traversal.
- Dirty repository state is rejected unless policy explicitly allows and records it.
- Missing required sources fail compilation; optional sources produce warnings.
- Budget truncation is explicit and deterministic after tie-breaking.

## Planning

The compile plan records selected SQL, FTS, vector, graph, memory, source, and observation lanes; their parameters; graph depth; limits; reranking; and evidence hydration. Plans are observable and included by digest in the compiled context.

## Error codes

Required stable codes include:

- <code>context_manifest_invalid</code>;
- <code>context_view_unknown</code>;
- <code>context_source_required_unavailable</code>;
- <code>context_revision_invalid</code>;
- <code>context_dirty_state_denied</code>;
- <code>context_authorization_denied</code>;
- <code>context_budget_invalid</code>;
- <code>context_compile_failed</code>;
- <code>context_not_found</code>;
- <code>context_materialization_failed</code>;
- <code>context_parent_mismatch</code>.

## Idempotency

A caller MAY provide an idempotency key. Repeating a successful request against the same manifest digest, canonical store snapshots, plan version, and authorization context SHOULD return the same context or an equivalent cached generation.
